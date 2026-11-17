// The ledger chain check from wraithkit/src/ledger.rs and verify.rs, in the browser.
// Runs in browsers and in Node 20+ (both have crypto.subtle). Checked by tools/check-chain.mjs.

// Rust sorts map keys by UTF-8 bytes, which is the same order as code points.
const byCodePoint = (a, b) => {
  const x = Array.from(a, (c) => c.codePointAt(0));
  const y = Array.from(b, (c) => c.codePointAt(0));
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i] - y[i];
  return x.length - y.length;
};

// Same bytes as `serde_json::to_value(x)?.to_string()` for the integer-only data WraithKit hashes.
// Reject numbers whose original JSON bytes cannot be reproduced safely in JavaScript.
export function canonical(v) {
  if (typeof v === 'number' && (!Number.isSafeInteger(v) || Object.is(v, -0))) throw new Error('This browser check supports safe integers only. Use the Rust verifier for this ledger.');
  if (v === null || typeof v !== 'object') return JSON.stringify(v);
  if (Array.isArray(v)) return `[${v.map(canonical).join(',')}]`;
  return `{${Object.keys(v).sort(byCodePoint).map((k) => `${JSON.stringify(k)}:${canonical(v[k])}`).join(',')}}`;
}

export async function sha256Hex(text) {
  const d = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(d), (b) => b.toString(16).padStart(2, '0')).join('');
}

export const lineHash = (l) => sha256Hex(canonical({ seq: l.seq, prev: l.prev, at_ms: l.at_ms, event: l.event }));

export const proposalDigest = (p) =>
  sha256Hex(canonical({ tool: p.tool, args: p.args, inputs: p.inputs, influenced_by: p.influenced_by }));

// Parses JSONL text the way Ledger::open does: a torn final line is noted, not failed.
export function parseLedger(text) {
  const raw = text.split('\n');
  if (raw.at(-1) === '') raw.pop();
  const lines = [];
  let torn = false;
  for (let i = 0; i < raw.length; i++) {
    try {
      lines.push(JSON.parse(raw[i]));
    } catch {
      if (i === raw.length - 1) { torn = true; break; }
      throw new Error(`line ${i + 1} is not JSON`);
    }
  }
  return { lines, torn };
}

// Returns one row per line plus the first break. `head` is the last stored hash.
export async function verifyChain(lines) {
  const rows = [];
  let brokenAt = -1;
  let prev = '';
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (!l || typeof l !== 'object' || Array.isArray(l) || !Number.isSafeInteger(l.seq) || !Number.isSafeInteger(l.at_ms) || typeof l.prev !== 'string' || typeof l.hash !== 'string' || !/^[a-f0-9]{64}$/.test(l.hash) || !l.event || typeof l.event !== 'object' || Array.isArray(l.event) || typeof l.event.kind !== 'string') throw new Error(`Line ${i + 1} is not a complete ledger record.`);
    let reason = '';
    if (l.seq !== i) reason = `seq is ${l.seq}, expected ${i}`;
    else if (l.prev !== prev) reason = 'prev does not match the line before';
    else if ((await lineHash(l)) !== l.hash) reason = 'hash does not match its content';
    if (reason && brokenAt < 0) brokenAt = i;
    rows.push({ seq: i, kind: l.event?.kind ?? '?', hash: l.hash, ok: !reason, reason, after: brokenAt >= 0 && brokenAt < i });
    prev = l.hash;
  }
  return { rows, brokenAt, head: lines.at(-1)?.hash ?? '' };
}
