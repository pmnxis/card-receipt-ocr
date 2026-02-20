// SPDX-FileCopyrightText: © 2025 Jinwoo Park (pmnxis@gmail.com)
//
// SPDX-License-Identifier: MIT

// Tesseract.js OCR bridge for WASM interop
// Pattern: chama-optics js/heif_helper.js

let tesseractWorker = null;

// Initialize tesseract worker with Korean + English
async function initWorker() {
    if (tesseractWorker) return;
    tesseractWorker = await Tesseract.createWorker(['kor', 'eng']);
    // Optimize for accuracy over speed
    await tesseractWorker.setParameters({
        preserve_interword_spaces: '1',
        user_defined_dpi: '300',
    });
}

// Preprocess image for better OCR accuracy:
// 1. Convert to grayscale
// 2. Scale up small images (Tesseract works best at 300+ DPI)
// 3. Otsu binarization (automatic threshold, proven with Tesseract)
async function preprocessForOcr(imageBytes) {
    return new Promise((resolve) => {
        const blob = new Blob([imageBytes]);
        const url = URL.createObjectURL(blob);
        const img = new Image();

        img.onload = () => {
            try {
                const canvas = document.createElement('canvas');
                const ctx = canvas.getContext('2d', { willReadFrequently: true });

                // Scale up small images (target: longest side >= 2000px)
                let scale = 1;
                const maxDim = Math.max(img.width, img.height);
                if (maxDim < 2000) {
                    scale = Math.ceil(2000 / maxDim);
                }

                canvas.width = img.width * scale;
                canvas.height = img.height * scale;

                // Use high-quality scaling
                ctx.imageSmoothingEnabled = true;
                ctx.imageSmoothingQuality = 'high';
                ctx.drawImage(img, 0, 0, canvas.width, canvas.height);

                const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
                const data = imageData.data;
                const len = data.length;

                // Pass 1: Convert to grayscale and build histogram
                const hist = new Uint32Array(256);
                for (let i = 0; i < len; i += 4) {
                    const g = Math.round(
                        0.299 * data[i] + 0.587 * data[i + 1] + 0.114 * data[i + 2]
                    );
                    data[i] = data[i + 1] = data[i + 2] = g;
                    hist[g]++;
                }

                // Otsu's method: find optimal binarization threshold
                const totalPixels = len / 4;
                let sum = 0;
                for (let t = 0; t < 256; t++) sum += t * hist[t];

                let sumB = 0, wB = 0, maxVar = 0, threshold = 128;
                for (let t = 0; t < 256; t++) {
                    wB += hist[t];
                    if (wB === 0) continue;
                    const wF = totalPixels - wB;
                    if (wF === 0) break;
                    sumB += t * hist[t];
                    const mB = sumB / wB;
                    const mF = (sum - sumB) / wF;
                    const variance = wB * wF * (mB - mF) ** 2;
                    if (variance > maxVar) {
                        maxVar = variance;
                        threshold = t;
                    }
                }

                // Pass 2: Apply binarization (pure black & white)
                // and count white vs black pixels to detect dark backgrounds
                let whiteCount = 0;
                for (let i = 0; i < len; i += 4) {
                    const bw = data[i] > threshold ? 255 : 0;
                    data[i] = data[i + 1] = data[i + 2] = bw;
                    if (bw === 255) whiteCount++;
                }

                // Pass 3: If image is predominantly dark (white text on dark bg),
                // invert so Tesseract gets black text on white background
                const blackCount = totalPixels - whiteCount;
                if (blackCount > whiteCount) {
                    console.log(`OCR: dark background detected (${blackCount} black vs ${whiteCount} white), inverting`);
                    for (let i = 0; i < len; i += 4) {
                        data[i] = data[i + 1] = data[i + 2] = 255 - data[i];
                    }
                }

                ctx.putImageData(imageData, 0, 0);

                canvas.toBlob(
                    (blob) => {
                        blob.arrayBuffer().then((buf) => resolve(new Uint8Array(buf)));
                    },
                    'image/png'
                );
            } catch (e) {
                console.warn('Image preprocessing failed, using original:', e);
                resolve(imageBytes);
            }
            URL.revokeObjectURL(url);
        };

        img.onerror = () => {
            URL.revokeObjectURL(url);
            resolve(imageBytes);
        };

        img.src = url;
    });
}

// Called from Rust: perform OCR on image bytes (Uint8Array)
// Returns recognized text string
export async function ocr_recognize(imageBytes) {
    await initWorker();
    const processed = await preprocessForOcr(imageBytes);
    console.log(
        `OCR: preprocessed ${imageBytes.length} -> ${processed.length} bytes`
    );
    const result = await tesseractWorker.recognize(processed);
    console.log('OCR result:', result.data.text.substring(0, 200));
    return result.data.text;
}

// Called from Rust: open file picker and return array of {name, bytes}
export function open_file_picker(queue_callback) {
    return new Promise((resolve) => {
        const input = document.createElement('input');
        input.type = 'file';
        input.multiple = true;
        input.accept = 'image/jpeg,image/png,image/jpg';
        input.onchange = async () => {
            const results = [];
            for (const file of input.files) {
                const buf = await file.arrayBuffer();
                results.push({ name: file.name, bytes: new Uint8Array(buf) });
            }
            resolve(results);
        };
        // If user cancels, resolve empty
        input.oncancel = () => resolve([]);
        input.click();
    });
}

// Called from Rust: trigger a file download in the browser
export function download_file(data, filename, mimeType) {
    const blob = new Blob([data], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    a.style.display = 'none';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(() => URL.revokeObjectURL(url), 5000);
}
