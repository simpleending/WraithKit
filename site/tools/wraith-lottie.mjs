// Writes assets/wraith.json, the hand-built Lottie of the footer and 404 wraith: `node site/tools/wraith-lottie.mjs`.
// 60 frames at 30 fps: the hem waves, the body bobs, the eyes blink once.
import { writeFileSync } from 'node:fs';

const BONE = [0.937, 0.906, 0.839, 1];
const SOOT = [0.047, 0.043, 0.059, 1];
const ease = (n) => ({ i: { x: Array(n).fill(0.45), y: Array(n).fill(1) }, o: { x: Array(n).fill(0.55), y: Array(n).fill(0) } });
const still = (k) => ({ a: 0, k });
const keys = (frames, n) => ({ a: 1, k: frames.map(([t, s], j) => (j < frames.length - 1 ? { t, s, ...ease(n) } : { t, s })) });

// Body: two straight sides, a dome, and a hem of four scallops. `hem` holds the 7 hem y values, right to left.
function body(hem) {
  const v = [[26, 196], [30, 100], [100, 30], [170, 100], [174, 196]];
  const i = [[0, 0], [0, 0], [-39, 0], [0, -39], [0, 0]];
  const o = [[0, 0], [0, -39], [39, 0], [0, 0], [0, 0]];
  hem.forEach((y, k) => { v.push([155 - k * 18.4, y]); i.push([8, 0]); o.push([-8, 0]); });
  return [{ i, o, v, c: true }];
}
const wave = [214, 196, 214, 196, 214, 196, 214];
const back = [198, 212, 198, 212, 198, 212, 198];

const tr = (extra = {}) => ({ ty: 'tr', p: still([0, 0]), a: still([0, 0]), s: still([100, 100]), r: still(0), o: still(100), sk: still(0), sa: still(0), ...extra });
const fill = (c) => ({ ty: 'fl', c: still(c), o: still(100), r: 1 });
const ellipse = (p, s) => ({ ty: 'el', p: still(p), s: still(s), d: 1 });

const shapes = [{
  ty: 'gr', nm: 'float',
  it: [
    { ty: 'gr', nm: 'face', it: [
      { ty: 'gr', nm: 'eyes', it: [ellipse([78, 118], [18, 28]), ellipse([122, 118], [18, 28]), fill(SOOT),
        tr({ a: still([100, 118]), p: still([100, 118]), s: keys([[0, [100, 100]], [38, [100, 100]], [42, [100, 8]], [46, [100, 100]], [59, [100, 100]]], 2) })] },
      { ty: 'gr', nm: 'mouth', it: [ellipse([100, 156], [12, 18]), fill(SOOT), tr()] },
      tr(),
    ] },
    { ty: 'gr', nm: 'body', it: [{ ty: 'sh', ks: keys([[0, body(wave)], [30, body(back)], [59, body(wave)]], 1) }, fill(BONE), tr()] },
    tr({ p: keys([[0, [0, 4]], [30, [0, -6]], [59, [0, 4]]], 1) }),
  ],
}];

const lottie = {
  v: '5.9.0', fr: 30, ip: 0, op: 60, w: 200, h: 240, nm: 'wraith', ddd: 0, assets: [],
  layers: [{ ddd: 0, ind: 1, ty: 4, nm: 'wraith', sr: 1, ao: 0, ip: 0, op: 60, st: 0, bm: 0,
    ks: { o: still(100), r: still(0), p: still([0, 0, 0]), a: still([0, 0, 0]), s: still([100, 100, 100]) }, shapes }],
};
writeFileSync(new URL('../assets/wraith.json', import.meta.url), JSON.stringify(lottie));
console.log('wrote assets/wraith.json');
