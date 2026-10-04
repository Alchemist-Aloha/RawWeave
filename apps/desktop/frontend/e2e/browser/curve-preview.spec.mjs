import { $, browser, expect } from '@wdio/globals';

async function draft(field, text) {
  await browser.execute((element, next) => {
    const prototype = element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, next);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  }, field, text);
}

describe('transfer curve controls', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => localStorage.clear());
    await browser.refresh();
    await $('input[placeholder="Search nodes"]').waitForDisplayed();
  });

  it('shows reversed ranges, clamped tails and invalid bounds without removing exact fields', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await draft(search, 'core.map-range');
    await $('.node-library__item').click();
    const node = await $('[aria-label="Map Range node"]');
    await node.$('summary').click();
    const edit = async (label, text) => {
      const field = await node.$(`input[aria-label="${label}"]`);
      await draft(field, text);
      await field.click();
      await browser.keys('Enter');
    };
    await edit('Input Minimum', '1');
    await expect(node.$('.parameter-transfer-preview svg')).not.toExist();
    await edit('Input Maximum', '0');
    await expect(node.$('.parameter-transfer-preview svg')).toBeDisplayed();
    const trace = await node.$('.parameter-transfer-preview polyline').getAttribute('points');
    await node.$('input[type="checkbox"]').click();
    await expect(node.$('.parameter-transfer-preview')).toHaveText(expect.stringContaining('Clamped'));
    expect(await node.$('.parameter-transfer-preview polyline').getAttribute('points')).not.toBe(trace);
    await expect(node.$('input[aria-label="Input Minimum"]')).toHaveValue('1');
    await expect(node.$('input[aria-label="Input Maximum"]')).toHaveValue('0');
  });

  it('adds and shapes a point with a trusted click-drag and one-step Undo', async () => {
    await draft(await $('input[placeholder="Search nodes"]'), 'core.curve');
    await browser.execute(() => [...document.querySelectorAll('.node-library__item')].find((item) => item.textContent.includes('core.curve') && !item.textContent.includes('core.curves')).click());
    const node = await $('[aria-label="Curve node"]');
    await node.$('summary').click();
    await $('.react-flow__controls-fitview').click();
    await browser.pause(300);
    const svg = await node.$('.curve-editor svg');
    const coords = await browser.execute((element) => {
      const m = element.getScreenCTM();
      return [60, 39.2].map((y) => ({ x: Math.round(m.a * 100 + m.c * y + m.e), y: Math.round(m.b * 100 + m.d * y + m.f) }));
    }, svg);
    const before = await $('.workflow-health').getAttribute('title');
    await browser.execute((element) => {
      window.__curveHeldHashes = [];
      element.addEventListener('pointermove', (event) => {
        if (event.buttons === 1) queueMicrotask(() => window.__curveHeldHashes.push(document.querySelector('.workflow-health').title));
      });
    }, svg);
    await browser.performActions([{ type: 'pointer', id: 'curve-mouse', parameters: { pointerType: 'mouse' }, actions: [
      { type: 'pointerMove', duration: 100, origin: 'viewport', ...coords[0] },
      { type: 'pointerDown', button: 0 },
      { type: 'pointerMove', duration: 200, origin: 'viewport', ...coords[1] },
      { type: 'pointerUp', button: 0 },
    ] }]);
    await browser.releaseActions();
    const held = await browser.execute(() => window.__curveHeldHashes);
    expect(held.length).toBeGreaterThan(0);
    expect(held.every((hash) => hash === before)).toBe(true);
    await browser.waitUntil(async () => (await $('.workflow-health').getAttribute('title')) !== before);
    expect((await node.$$('.curve-editor__point')).length).toBe(3);
    const text = await node.$('textarea').getValue();
    expect(Number(text.split(';')[1].split(',')[1])).toBeCloseTo(0.7, 1);
    await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
    await expect(node.$('textarea')).toHaveValue('0,0;1,1');
    expect((await node.$$('.curve-editor__point')).length).toBe(2);
  });

  it('shows point curves outside Advanced and previews edits before applying', async () => {
    await draft(await $('input[placeholder="Search nodes"]'), 'core.curve');
    await browser.execute(() => [...document.querySelectorAll('.node-library__item')].find((item) => item.textContent.includes('core.curve') && !item.textContent.includes('core.curves')).click());
    const node = await $('[aria-label="Curve node"]');
    await node.$('summary').click();
    const field = await node.$('textarea[aria-label="Curve Points"]');
    await expect(field).toBeDisplayed();
    const initial = await node.$('.curve-preview polyline').getAttribute('points');
    await draft(field, '0,0;0.5,0.7;1,1');
    expect(await node.$('.curve-preview polyline').getAttribute('points')).not.toBe(initial);
    await expect(node.$('[aria-label="Reset Curve Points"]')).not.toExist();
    await field.click();
    await browser.keys('Enter');
    await expect(node.$('[aria-label="Reset Curve Points"]')).toExist();
    await draft(field, '0,0;0,1');
    await expect(field).toHaveAttribute('aria-invalid', 'true');
    await field.click();
    await browser.keys('Escape');
    await expect(field).toHaveValue('0,0;0.5,0.7;1,1');
  });

  it('previews the gamma slider draft and restores the neutral diagonal on Escape', async () => {
    await draft(await $('input[placeholder="Search nodes"]'), 'core.curves');
    await $('.node-library__item').click();
    const node = await $('[aria-label="Curves node"]');
    await node.$('summary').click();
    const slider = await node.$('input[type="range"]');
    const initial = await node.$('.curve-preview polyline').getAttribute('points');
    await draft(slider, '2');
    expect(await node.$('.curve-preview polyline').getAttribute('points')).not.toBe(initial);
    await browser.execute((element) => element.focus(), slider);
    await browser.keys('Escape');
    expect(await node.$('.curve-preview polyline').getAttribute('points')).toBe(initial);
  });
});
