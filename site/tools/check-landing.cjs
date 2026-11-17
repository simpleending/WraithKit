// Run against a local preview: node site/tools/check-landing.cjs http://127.0.0.1:8797/
// Requires Playwright. PLAYWRIGHT_MODULE and PLAYWRIGHT_CHROMIUM may point to an existing install.
// Set CHECK_CDN=1 to require the optional CDN animations; normal checks block the CDN deliberately.
const assert = require('node:assert/strict');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const url = process.argv[2] || 'http://127.0.0.1:8797/';
const browserOptions = { headless: true };
if (process.env.PLAYWRIGHT_CHROMIUM) browserOptions.executablePath = process.env.PLAYWRIGHT_CHROMIUM;

(async () => {
  const browser = await chromium.launch(browserOptions);
  try {
    const result = [];
    const state = (page, text) => page.waitForFunction((value) => document.querySelector('[data-report-state]').textContent === value, text);
    const pageFor = async (options = {}) => {
      const page = await browser.newPage({ reducedMotion: 'reduce', ...options });
      page.errors = [];
      page.on('pageerror', (error) => page.errors.push(error.message));
      await page.route('https://cdn.jsdelivr.net/**', (route) => route.abort());
      return page;
    };
    const p = await pageFor({ viewport: { width: 1440, height: 1000 } });
    await p.goto(url, { waitUntil: 'networkidle' });
    await state(p, 'chain ok');
    for (const [step, expected] of [[1, 'broken at line 16'], [2, 'broken at line 17'], [3, 'chain ok, head changed'], [0, 'chain ok']]) {
      await p.locator(`[data-tamper="${step}"]`).click();
      await state(p, expected);
    }
    result.push('all four ledger states');
    assert.equal(await p.locator('.apps--primary > article').count(), 2);
    assert.match(await p.locator('#security-wraith').innerText(), /does not connect to 1-SEC or redact secrets/);
    assert.match(await p.locator('#release-wraith').innerText(), /does not rerun CI, build or publish/);
    assert.equal(await p.locator('.bonus__app').count(), 2);
    assert.match(await p.locator('.try__availability').innerText(), /Local preview/);
    assert.match(await p.locator('.try__availability').innerText(), /Kiro and later ChatGPT-assisted/);
    assert.equal(await p.locator('a[href*="kidbuu00"]').count(), 0);
    const primaryCommands = await p.locator('.try > .term code').allTextContents();
    assert.deepEqual(primaryCommands, [
      'cargo build --workspace --locked',
      'cargo run --locked --offline -p security-wraith -- demo',
      'cargo run --locked --offline -p release-wraith -- demo',
    ]);
    await p.locator('.bonus-commands summary').click();
    assert.equal(await p.getByRole('button', { name: 'Copy Repo demo command' }).isVisible(), true);
    await p.locator('.bonus-commands summary').click();
    result.push('primary/bonus hierarchy, exact runnable commands and honest capability boundaries');

    const fixture = await (await p.request.get(new URL('assets/ledger.jsonl', url).href)).body();
    const upload = async (buffer) => p.locator('[data-own]').setInputFiles({ name: 'test.jsonl', mimeType: 'text/plain', buffer });
    await upload(fixture);
    await p.waitForFunction(() => document.querySelector('[data-known-head]').textContent === 'none written down for this file');
    for (const buffer of [Buffer.from('null\n'), Buffer.from('{}\n'), Buffer.from(''), Buffer.alloc(2 * 1024 * 1024 + 1, 'x')]) {
      await upload(buffer);
      await state(p, 'File could not be checked');
      assert.equal(await p.locator('[data-chain] > li').count(), 0, 'failed upload clears the old rows');
      assert.equal(await p.locator('[data-report-head]').textContent(), 'not checked');
      assert.equal(await p.locator('[data-known-head]').textContent(), 'not checked');
      assert.equal(await p.locator('[data-tamper="0"]').isEnabled(), true, 'restore is available');
    }
    await p.locator('[data-tamper="0"]').click();
    await state(p, 'chain ok');
    assert.equal(await p.locator('[data-chain] > li').count(), 25);
    assert.deepEqual(p.errors, []);
    result.push('valid, malformed, empty and oversized uploads; restore');
    await p.close();

    for (const width of [320, 360, 390, 768, 1440]) {
      const page = await pageFor({ viewport: { width, height: 900 } });
      await page.goto(url, { waitUntil: 'networkidle' });
      await state(page, 'chain ok');
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth), width, `${width}px has no page overflow`);
      assert.equal(await page.locator('.hero__art').evaluate((image) => image.naturalWidth), 1536);
      await page.locator('.hero__primary').click();
      assert.equal(await page.evaluate(() => document.querySelector('#try-h').getBoundingClientRect().top >= document.querySelector('.bar').getBoundingClientRect().bottom), true, `${width}px Run it heading clears fixed bar`);
      await page.locator('.bonus-commands summary').focus();
      await page.keyboard.press('Enter');
      assert.equal(await page.getByRole('button', { name: 'Copy Repo demo command' }).isVisible(), true);
      await page.keyboard.press('Enter');

      await page.locator('footer').scrollIntoViewIfNeeded();
      assert.match(await page.locator('.foot__ghost').evaluate((el) => getComputedStyle(el).backgroundImage), /wraith-still\.svg/);
      if (process.env.LANDING_SCREENSHOTS) {
        await page.evaluate(() => scrollTo(0, 0));
        await page.screenshot({ path: `${process.env.LANDING_SCREENSHOTS}/landing-${width}.png`, fullPage: true });
      }
      assert.deepEqual(page.errors, []);
      await page.close();
    }
    result.push('320/360/390/768/1440px with CDN blocked, existing ghost fallback');

    const failed = await pageFor();
    await failed.route('**/assets/ledger.jsonl', (route) => route.fulfill({ status: 503, body: 'Unavailable' }));
    await failed.goto(url, { waitUntil: 'networkidle' });
    await state(failed, 'Demo ledger not loaded');
    assert.equal(await failed.locator('[data-tamper]:enabled').count(), 0);
    assert.equal(await failed.locator('[data-own]').isDisabled(), true);
    assert.equal(await failed.locator('[data-chain] > li').count(), 0);
    assert.deepEqual(failed.errors, []);
    await failed.close();
    result.push('missing bundled ledger fails closed');

    const nojs = await pageFor({ javaScriptEnabled: false });
    await nojs.goto(url, { waitUntil: 'networkidle' });
    assert.equal(await nojs.locator('[data-report-state]').textContent(), 'Not checked yet');
    assert.equal(await nojs.locator('[data-tamper]:enabled').count(), 0);
    await nojs.keyboard.press('Tab');
    assert.equal(await nojs.locator('.skip').evaluate((el) => el === document.activeElement), true);
    await nojs.close();
    result.push('no JavaScript never claims verification; keyboard skip link');

    const missing = await pageFor({ viewport: { width: 320, height: 740 } });
    await missing.goto(new URL('404.html', url).href, { waitUntil: 'networkidle' });
    assert.equal(await missing.locator('h1').textContent(), 'This page went stale.');
    assert.match(await missing.locator('.lost__ghost').evaluate((el) => getComputedStyle(el).backgroundImage), /wraith-still\.svg/);
    assert.equal(await missing.evaluate(() => document.documentElement.scrollWidth), 320);
    assert.deepEqual(missing.errors, []);
    await missing.close();
    result.push('404 page works at 320px without CDN');

    if (process.env.CHECK_CDN === '1') {
      const full = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
      const errors = [];
      full.on('pageerror', (e) => errors.push(e.message));
      await full.goto(url, { waitUntil: 'networkidle' });
      assert.equal(await full.evaluate(() => !!(window.gsap && window.ScrollTrigger && window.SplitText && window.Lenis)), true, 'CDN libraries load');
      await state(full, 'chain ok');
      await full.locator('footer').scrollIntoViewIfNeeded();
      await full.locator('.foot__ghost.is-ready svg').waitFor();
      assert.equal(await full.locator('.foot__ghost').evaluate((el) => getComputedStyle(el).backgroundImage), 'none');
      assert.deepEqual(errors, []);
      await full.close();
      result.push('full motion and existing Lottie footer loaded');
    }
    console.log(JSON.stringify({ passed: result }, null, 2));
  } finally { await browser.close(); }
})().catch((error) => { console.error(error); process.exitCode = 1; });
