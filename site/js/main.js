// WraithKit landing page. Optional motion enhances a readable, working static page.
import { parseLedger, verifyChain, lineHash } from './chain.js';

const { gsap, ScrollTrigger, SplitText, Lenis } = window;
const G = Boolean(gsap && ScrollTrigger && SplitText);
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
const small = matchMedia('(max-width: 760px)').matches;
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const hexDiff = (now, base) => [...now].map((c, i) => (c === base[i] ? c : `<span class="x">${c}</span>`)).join('');

// ---------- engine
let lenis = null;
if (G) {
  gsap.registerPlugin(ScrollTrigger, SplitText);
  if (!reduced && Lenis) {
    lenis = new Lenis({ autoRaf: false, anchors: true });
    lenis.on('scroll', ScrollTrigger.update);
    gsap.ticker.add((t) => lenis.raf(t * 1000));
    gsap.ticker.lagSmoothing(0);
  }
  const bar = $('[data-bar]');
  ScrollTrigger.create({ start: 0, end: 'max', onUpdate: (s) => bar.classList.toggle('is-hidden', s.direction === 1 && s.scroll() > 240) });
  bar.addEventListener('focusin', () => bar.classList.remove('is-hidden'));
  $$('.bar__nav a').forEach((a) => {
    const sec = $(a.getAttribute('href'));
    if (sec) ScrollTrigger.create({ trigger: sec, start: 'top 55%', end: 'bottom 55%', onToggle: (s) => (s.isActive ? a.setAttribute('aria-current', 'true') : a.removeAttribute('aria-current')) });
  });
  document.fonts?.ready.then(() => ScrollTrigger.refresh());
}

// ---------- reveals
if (G && !reduced) {
  SplitText.create('[data-intro-lede]', { type: 'lines', mask: 'lines', autoSplit: true, onSplit: (s) => gsap.from(s.lines, { yPercent: -125, duration: 1.1, ease: 'expo.out', stagger: 0.08, delay: 0.25 }) });

  $$('[data-split]').forEach((el) => SplitText.create(el, { type: 'lines', mask: 'lines', autoSplit: true,
    onSplit: (s) => gsap.from(s.lines, { yPercent: -125, duration: 1.15, stagger: 0.08, ease: 'expo.out', scrollTrigger: { trigger: el, start: 'top 86%', once: true } }) }));
  $$('[data-reveal]').forEach((el) => gsap.from(el, { y: -18, autoAlpha: 0, duration: 1.1, ease: 'expo.out', scrollTrigger: { trigger: el, start: 'top 90%', once: true } }));
  $$('[data-scrub-words]').forEach((el) => SplitText.create(el, { type: 'words', autoSplit: true,
    onSplit: (s) => gsap.fromTo(s.words, { opacity: 0.3 }, { opacity: 1, stagger: 0.05, ease: 'none', scrollTrigger: { trigger: el, start: 'top 82%', end: 'bottom 55%', scrub: true } }) }));

  gsap.from('[data-spine] li', { y: -24, autoAlpha: 0, duration: 1, stagger: 0.055, ease: 'expo.out', scrollTrigger: { trigger: '[data-spine]', start: 'top 85%', once: true } });
}

// ---------- shared demo ledger
const ledgerText = fetch('assets/ledger.jsonl').then((r) => (r.ok ? r.text() : Promise.reject(new Error(`ledger: HTTP ${r.status}`))));
// ---------- the ledger: verify, then try to rewrite history
const chainEl = $('[data-chain]');
const report = $('.report');
const reportState = $('[data-report-state]'), reportText = $('[data-report-text]'), reportHead = $('[data-report-head]'), knownHeadEl = $('[data-known-head]');
const steps = $$('[data-tamper]');
const detail = (e) => ({
  run_started: e.app, model_turn: e.message?.tool_calls ? `${e.message.tool_calls.length} tool call${e.message.tool_calls.length > 1 ? 's' : ''}` : 'reply',
  tool_result: `${e.tool} ${e.ok ? 'ok' : 'failed'}`, evidence: `${e.id} ${e.source}${e.flags?.length ? ' (!)' : ''}`, claim: e.status,
  proposed: `${e.id} ${e.tool}${e.warnings?.length ? ' (!)' : ''}`, decided: `${e.proposal} ${e.approve ? 'approved' : 'denied'}`,
  effect_started: e.proposal, effect_done: `${e.proposal} ${e.ok ? 'done' : 'failed'}`, blocked: `${e.tool} blocked`, run_finished: e.status, run_resumed: e.after,
})[e.kind] ?? '';

async function renderChain(lines, known, opts = {}) {
  const res = await verifyChain(lines);
  chainEl.innerHTML = res.rows.map((r, i) => {
    const l = lines[i];
    const cls = [!r.ok && 'is-bad', r.after && 'is-after', opts.moved?.has(i) && 'is-moved'].filter(Boolean).join(' ');
    return `<li class="${cls}"><details><summary><span class="sq">${i}</span><span class="kd">${esc(r.kind)}<small>${esc(detail(l.event ?? {}))}</small></span><span class="hs">${esc(String(r.hash).slice(0, 8))}</span>${r.reason ? `<span class="why">${esc(r.reason)}</span>` : ''}</summary><pre>${esc(JSON.stringify(l, null, 1))}</pre></details></li>`;
  }).join('');
  knownHeadEl.textContent = known || 'none written down for this file';
  reportHead.innerHTML = known ? hexDiff(res.head, known) : esc(res.head);
  const broken = res.brokenAt >= 0, rewritten = !broken && known && res.head !== known;
  report.dataset.state = broken ? 'broken' : rewritten ? 'rewritten' : 'ok';
  reportState.textContent = broken ? `broken at line ${res.brokenAt}` : rewritten ? 'chain ok, head changed' : 'chain ok';
  reportText.textContent = broken
    ? `Line ${res.brokenAt}: ${res.rows[res.brokenAt].reason}. verify stops here and fails the run.`
    : rewritten
      ? 'Every link checks out again. Only the head gives it away, so compare it with the head you kept. That is what tamper-evident means here.'
      : `${lines.length} lines, each one linked to the line before.${opts.torn ? ' The torn last line was ignored, as verify does.' : ''}`;
  if (G && !reduced && opts.flash) gsap.fromTo($$('li.is-bad, li.is-after, li.is-moved', chainEl), { x: -8 }, { x: 0, duration: 0.7, stagger: 0.015, ease: 'expo.out' });
  return res;
}

const ownInput = $('[data-own]');
const maxLedgerBytes = 2 * 1024 * 1024;
const clearResult = (state, message) => {
  chainEl.replaceChildren();
  reportHead.textContent = 'not checked';
  knownHeadEl.textContent = 'not checked';
  report.dataset.state = 'broken';
  reportState.textContent = state;
  reportText.textContent = message;
};
reportState.textContent = 'Checking demo ledger…';
reportText.textContent = 'Verifying the bundled demo in your browser.';

ledgerText.then(async (text) => {
  const real = parseLedger(text).lines;
  if (!real.length) throw new Error('The bundled ledger is empty.');
  const known = real.at(-1).hash;
  const target = real.findIndex((l) => l.event.kind === 'decided' && l.event.proposal === 'P2');
  if (target < 0) throw new Error('The bundled ledger is missing the demo decision.');
  let cur = structuredClone(real);
  let busy = false;
  const setStep = (n) => steps.forEach((b) => {
    const k = +b.dataset.tamper;
    b.disabled = k === 0 ? n === 0 : k !== n + 1;
    b.classList.toggle('is-done', k > 0 && k <= n);
  });
  const lock = () => {
    busy = true;
    steps.forEach((b) => { b.disabled = true; });
    ownInput.disabled = true;
    report.setAttribute('aria-busy', 'true');
  };
  const unlock = (step) => {
    busy = false;
    setStep(step);
    ownInput.disabled = false;
    report.removeAttribute('aria-busy');
  };
  await renderChain(cur, known);
  unlock(0);
  if (G && !reduced) gsap.from($$('li', chainEl), { y: -10, autoAlpha: 0, duration: 0.9, stagger: 0.025, ease: 'expo.out', scrollTrigger: { trigger: chainEl, start: 'top 80%', once: true } });
  steps.forEach((b) => b.addEventListener('click', async () => {
    if (busy) return;
    const k = +b.dataset.tamper;
    const keepFocus = document.activeElement === b;
    const moved = new Set();
    lock();
    try {
      if (k === 0) cur = structuredClone(real);
      if (k === 1) cur[target].event.approve = true;
      if (k === 2) { cur[target].hash = await lineHash(cur[target]); moved.add(target); }
      if (k === 3) for (let i = target + 1; i < cur.length; i++) { cur[i].prev = cur[i - 1].hash; cur[i].hash = await lineHash(cur[i]); moved.add(i); }
      await renderChain(cur, known, { flash: true, moved });
      unlock(k);
      // Desktop: the panel is sticky, so bring the changed line next to it.
      const row = $('li.is-bad, li.is-moved', chainEl);
      if (row && !small) lenis ? lenis.scrollTo(row, { offset: -innerHeight / 2 }) : row.scrollIntoView({ block: 'center', behavior: reduced ? 'auto' : 'smooth' });
      const next = steps.find((s) => !s.disabled && +s.dataset.tamper === k + 1) ?? steps.find((s) => +s.dataset.tamper === 0);
      if (keepFocus && b.disabled) next?.focus();
    } catch (err) {
      clearResult('Check could not finish', err.message);
      unlock(-1);
    }
  }));
  ownInput.addEventListener('change', async (e) => {
    const file = e.target.files?.[0];
    if (!file || busy) return;
    lock();
    clearResult('Checking your ledger…', 'Verifying this file locally. Nothing is uploaded.');
    try {
      if (file.size > maxLedgerBytes) throw new Error('This browser demo accepts files up to 2 MiB. Use the Rust verifier for a larger ledger.');
      const { lines, torn } = parseLedger(await file.text());
      if (!lines.length) throw new Error('The file has no complete ledger lines.');
      await renderChain(lines, '', { torn });
      reportText.textContent += ' Compare the head with the one printed when the run ended.';
    } catch (err) {
      clearResult('File could not be checked', err.message);
    } finally {
      // Only Restore is offered for an uploaded file, including a rejected one.
      unlock(-1);
      e.target.value = '';
    }
  });
}).catch((e) => {
  clearResult('Demo ledger not loaded', e.message);
  steps.forEach((b) => { b.disabled = true; });
  ownInput.disabled = true;
});

// ---------- copy buttons
$$('[data-copy]').forEach((b) => b.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(b.previousElementSibling.textContent);
    b.textContent = 'Copied';
  } catch { b.textContent = 'Select it'; }
  b.classList.add('is-done');
  setTimeout(() => { b.textContent = 'Copy'; b.classList.remove('is-done'); }, 1600);
}));

// ---------- the Lottie wraith, loaded only when it is near, played on the shared clock
const ghost = $('[data-lottie]');
if (ghost) {
  const io = new IntersectionObserver((entries) => {
    if (!entries.some((en) => en.isIntersecting)) return;
    io.disconnect();
    const s = document.createElement('script');
    s.src = 'https://cdn.jsdelivr.net/npm/lottie-web@5.13.0/build/player/lottie_light.min.js';
    s.integrity = 'sha384-Gr3FGWSrOz4fzm9bvrWwhuQH87JMUCLlOTaHhpddbnlHHWCZPxMeQ3KUPsomzIii';
    s.crossOrigin = 'anonymous';
    s.onload = () => {
      const anim = window.lottie.loadAnimation({ container: ghost, renderer: 'svg', loop: true, autoplay: false, path: 'assets/wraith.json' });
      anim.addEventListener('DOMLoaded', () => {
        ghost.classList.add('is-ready');
        if (!G || reduced) return anim.goToAndStop(0, true);
        let on = true;
        new IntersectionObserver(([en]) => (on = en.isIntersecting)).observe(ghost);
        gsap.ticker.add((t) => on && anim.goToAndStop((t * 30) % 60, true));
      });
    };
    document.head.append(s);
  }, { rootMargin: '400px' });
  io.observe(ghost);
}
