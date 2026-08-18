import { readFileSync, statSync } from 'node:fs';
import { gzipSync } from 'node:zlib';

const artifact = new URL('../www/src/wasm/gallery_web_bg.wasm', import.meta.url);
const bytes = readFileSync(artifact);
const raw = statSync(artifact).size;
const gzip = gzipSync(bytes, { level: 9 }).length;

console.log(`gallery_web_bg.wasm raw:  ${raw.toLocaleString()} bytes`);
console.log(`gallery_web_bg.wasm gzip: ${gzip.toLocaleString()} bytes`);
