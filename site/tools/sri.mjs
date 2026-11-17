// Fills the integrity="sha384-..." attributes from the exact CDN bytes: `node site/tools/sri.mjs`.
// Rerun after changing a library version in index.html, 404.html or js/main.js.
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';

const files = ['../index.html', '../404.html', '../js/main.js'].map((f) => new URL(f, import.meta.url));
const cdn = /https:\/\/cdn\.jsdelivr\.net\/npm\/[^"']+/g;
const cache = new Map();
for (const file of files) {
  let text = readFileSync(file, 'utf8');
  for (const url of new Set(text.match(cdn) ?? [])) {
    if (!cache.has(url)) {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
      cache.set(url, `sha384-${createHash('sha384').update(Buffer.from(await res.arrayBuffer())).digest('base64')}`);
    }
    // The integrity value sits right after the URL, in the same tag or on the next line.
    const at = text.indexOf(url);
    const near = text.slice(at, at + url.length + 200);
    const fixed = near.replace(/sha384-[A-Za-z0-9+/=]+/, cache.get(url));
    text = text.slice(0, at) + fixed + text.slice(at + near.length);
  }
  writeFileSync(file, text);
}
for (const [u, h] of cache) console.log(h, u);
