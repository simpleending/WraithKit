// Regression tests for the real inline UI event handler, with synthetic DOM/network boundaries.
// Node built-ins only; no browser framework or dependency installation.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import vm from 'node:vm';
const html = await readFile(new URL('../../wraithkit/ui/index.html', import.meta.url), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
function ui(response) {
  const nodes = new Map();
  const node = () => ({textContent:'', disabled:false, hidden:true, children:[], handlers:{},
    addEventListener(name, handler) { this.handlers[name] = handler; },
    replaceChildren(...children) { this.children = children; },
    append(...children) { this.children.push(...children); }, setAttribute() {}, focus() {}});
  const document = {activeElement:null, createElement:node,
    getElementById(id) { if (!nodes.has(id)) nodes.set(id,node()); return nodes.get(id); }};
  vm.runInNewContext(script, {document, location:{pathname:'/synthetic-token/'}, setInterval() {},
    fetch(url) { return url.endsWith('/api/state') ? new Promise(()=>{}) : response(); }});
  return {get:id=>document.getElementById(id), verify:()=>document.getElementById('verify-btn').handlers.click()};
}
const good = () => Promise.resolve({ok:true,json:async()=>({all_ok:true,checks:[{ok:true,name:'chain',detail:'synthetic'}],head:'f'.repeat(64)})});
const healthy = ui(good);
await healthy.verify();
assert.equal(healthy.get('verify-btn').disabled,false);
assert.equal(healthy.get('live').textContent,'Verification passed.');
assert.equal(healthy.get('verify-section').hidden,false);
for (const [name,response] of [
  ['network failure',()=>Promise.reject(Error('offline'))],
  ['HTTP error',()=>Promise.resolve({ok:false,status:500,json:async()=>({error:'unavailable'})})],
  ['malformed JSON',()=>Promise.resolve({ok:true,json:async()=>{throw Error('bad json');}})],
]) {
  let fail = false;
  const instance = ui(()=>fail ? response() : good());
  await instance.verify(); // Previous successful result must not remain current after failure.
  fail = true;
  await assert.doesNotReject(instance.verify(), name);
  assert.equal(instance.get('verify-btn').disabled,false, `${name}: retry remains available`);
  assert.equal(instance.get('verify-section').hidden,true, `${name}: stale successful panel hidden`);
  assert.match(instance.get('live').textContent,/could not complete/i, name);
  fail = false;
  await instance.verify();
  assert.equal(instance.get('live').textContent,'Verification passed.', `${name}: retry succeeds`);
}
console.log('shared UI verification: success, network/HTTP/JSON failure, stale-result hiding and retry passed');
