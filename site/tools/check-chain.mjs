// Self-check for js/chain.js against the real demo ledger: `node site/tools/check-chain.mjs`.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { canonical, parseLedger, proposalDigest, verifyChain, lineHash } from '../js/chain.js';

assert.equal(canonical({ b: 1, a: { d: 2, c: 3 }, 10: 0, 9: 0 }), '{"10":0,"9":0,"a":{"c":3,"d":2},"b":1}');

const text = readFileSync(new URL('../assets/ledger.jsonl', import.meta.url), 'utf8');
const { lines, torn } = parseLedger(text);
assert.equal(torn, false);
const ok = await verifyChain(lines);
assert.equal(ok.brokenAt, -1, 'the real ledger verifies');
assert.equal(ok.head, '6a45896461392e97a30492445c007bb454cf69f104568248610821c43b6121df', 'head printed by `verify`');

for (const l of lines.filter((l) => l.event.kind === 'proposed')) {
  assert.equal(await proposalDigest(l.event), l.event.digest, `digest of ${l.event.id}`);
}

// Tamper: flip the denial of P2 into an approval.
const i = lines.findIndex((l) => l.event.kind === 'decided' && l.event.proposal === 'P2');
const bad = structuredClone(lines);
bad[i].event.approve = true;
assert.equal((await verifyChain(bad)).brokenAt, i, 'edited content is caught at that line');
bad[i].hash = await lineHash(bad[i]);
assert.equal((await verifyChain(bad)).brokenAt, i + 1, 're-hashed line is caught at the next prev');
for (let k = i + 1; k < bad.length; k++) { bad[k].prev = bad[k - 1].hash; bad[k].hash = await lineHash(bad[k]); }
const rewritten = await verifyChain(bad);
assert.equal(rewritten.brokenAt, -1, 'a full rewrite is self-consistent');
assert.notEqual(rewritten.head, ok.head, 'but the head hash changes');

assert.equal(parseLedger(text + '{"seq":').torn, true, 'torn tail is noted');
console.log(`chain.js ok: ${lines.length} lines, tampered line ${i}`);

// Uploaded JSON must not show stale results or pretend unsupported numbers were checked.
await assert.rejects(verifyChain([null]), /complete ledger record/);
await assert.rejects(verifyChain([{}]), /complete ledger record/);
assert.throws(() => canonical({ amount: 1.5 }), /safe integers only/);
assert.throws(() => canonical({ amount: 9007199254740992 }), /safe integers only/);
assert.throws(() => canonical({ amount: -0 }), /safe integers only/);
console.log('uploaded-ledger validation ok');

// Hash-change highlights are nested spans, not the fixed-width row labels.
const reportCss = readFileSync(new URL('../css/style.css', import.meta.url), 'utf8');
assert.match(reportCss, /\.report__head > span, \.report__known > span/);
assert.doesNotMatch(reportCss, /\.report__(?:head|known) span\s*[,\{]/);
const landingHtml = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const sectionOrder = [...landingHtml.matchAll(/<section[^>]+id="([^"]+)"/g)].map((m) => m[1]);
assert.deepEqual(sectionOrder, ['top', 'crew', 'bridge', 'rule', 'ledger', 'try']);
console.log('ledger label scope and approval-section placement ok');

// Keep the requested links simple while retaining accessible names and focus cues.
assert.match(landingHtml, /<a class="bar__gh" href="https:\/\/github\.com\/simpleending\/WraithKit" aria-label="[^"]+"><svg/);
assert.match(landingHtml, /<a class="hero__primary" href="#try">Run the offline demos<\/a>/);
assert.doesNotMatch(landingHtml, /Two different jobs\.|A README or web page can tell an agent/);
assert.doesNotMatch(landingHtml, /kidbuu00\/wraithTest/);
assert.match(landingHtml, /Local preview/);
console.log('landing link, button and copy contracts ok');

assert.doesNotMatch(landingHtml, /0[12] \/ (Security|Release) Wraith/);
assert.match(landingHtml, /class="app__run" href="#try">Run Security Wraith<\/a>/);
assert.match(landingHtml, /class="app__run" href="#try">Run Release Wraith<\/a>/);
assert.match(landingHtml, /assets\/wraith-bridge\.webp/);
