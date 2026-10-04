import { $, browser, expect } from '@wdio/globals';

async function setNodeSearch(value) {
  const search = await $('input[placeholder="Search nodes"]');
  await browser.execute((element, next) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    setter.call(element, next);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  }, search, value);
  await expect(search).toHaveValue(value);
}

async function addNode(name) {
  await setNodeSearch(name);
  await $('.node-library__item').click();
  await setNodeSearch('');
}

async function openDetails(name) {
  const node = await $(`[aria-label="${name} node"]`);
  const details = await node.$('details.graph-node__details');
  if (!(await details.getProperty('open'))) await details.$('summary').click();
  await browser.waitUntil(async () => details.getProperty('open'));
  return node;
}

describe('node parameter usability', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => window.localStorage.clear());
    await browser.refresh();
    await $('input[placeholder="Search nodes"]').waitForDisplayed();
  });

  it('browses task categories and searches inside a collapsed group', async () => {
    const labels = await browser.execute(() => [...document.querySelectorAll('.node-library__group-summary > span:first-child')].map((el) => el.textContent));
    expect(labels).toContain('Tone & exposure');
    expect(labels).toContain('Mask sources');
    expect(labels).toContain('Mask combine');
    expect(labels).not.toContain('Core');
    const group = await $('summary*=Tone & exposure');
    await group.click();
    await expect(await group.parentElement()).not.toHaveAttribute('open');
    await setNodeSearch('tone & exposure');
    await expect(await group.parentElement()).toHaveAttribute('open');
    await expect($('.node-library__item')).toBeDisplayed();
    await browser.saveScreenshot('./logs/node-categories.png');
    await setNodeSearch('Exposure');
    await $('.node-library__item').click();
    await expect($('[aria-label="Exposure node"]')).toBeDisplayed();
  });

  it('keeps bounded sliders and reset controls usable', async () => {
    await addNode('Blur');
    const node = await openDetails('Blur');
    const slider = await node.$('input[aria-label="Blur Radius slider"]');
    await expect(slider).toBeDisplayed();
    await expect(slider).toHaveAttribute('min', '0');
    await expect(slider).toHaveAttribute('max', '20');
    await expect(slider).toHaveAttribute('step', '1');
    await browser.execute((element, next) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(element, next);
      element.dispatchEvent(new Event('input', { bubbles: true }));
      element.dispatchEvent(new Event('change', { bubbles: true }));
    }, slider, '12');
    await browser.execute((element) => element.dispatchEvent(new MouseEvent('mouseup', { bubbles: true })), slider);
    await expect(node.$('input[aria-label="Blur Radius"]')).toHaveValue('12');
    const reset = await node.$('button[aria-label="Reset Blur Radius"]');
    await browser.execute((element) => element.click(), reset);
    await expect(node.$('input[aria-label="Blur Radius"]')).toHaveValue('1');
    await expect(slider).toHaveValue('1');
  });

  it('changes a range only while pressed and dragged, never on hover', async () => {
    await addNode('Exposure');
    const node = await openDetails('Exposure');
    await $('.react-flow__controls-fitview').click();
    const slider = await node.$('input[aria-label="Exposure slider"]');
    const field = await node.$('input[aria-label="Exposure"]');
    const initial = await slider.getValue();
    const rect = await browser.execute((element) => {
      const bounds = element.getBoundingClientRect();
      return { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height };
    }, slider);
    const move = fraction => ({ type: 'pointerMove', duration: 100, origin: 'viewport', x: Math.round(rect.x + 5 + fraction * (rect.width - 10)), y: Math.round(rect.y + rect.height / 2) });
    await browser.execute((element) => {
      window.__sliderInputs = [];
      window.__sliderButtons = 0;
      document.addEventListener('pointermove', (event) => { window.__sliderButtons = event.buttons; }, true);
      element.addEventListener('input', () => window.__sliderInputs.push({ value: element.value, buttons: window.__sliderButtons }));
    }, slider);
    // Keep the complete gesture in one protocol call; some drivers release
    // pressed input sources between calls.
    await browser.performActions([{ type: 'pointer', id: 'slider-mouse', parameters: { pointerType: 'mouse' }, actions: [
      move(0.2), move(0.8), move(0.5), { type: 'pointerDown', button: 0 },
      move(0.65), move(0.8), { type: 'pointerUp', button: 0 }, move(0.1), move(0.9),
    ] }]);
    const samples = await browser.execute(() => window.__sliderInputs);
    expect(samples.length).toBeGreaterThan(0);
    expect(samples.every(sample => sample.buttons === 1)).toBe(true);
    const dragged = samples.at(-1).value;
    expect(Number(dragged)).toBeGreaterThan(Number(initial));
    await expect(field).toHaveValue(dragged);
    await expect(slider).toHaveValue(dragged);
    await expect(node.$('button[aria-label="Reset Exposure"]')).toExist();
    await browser.releaseActions();
  });

  it('keeps range drafts synchronized and applies only on key release', async () => {
    await addNode('Exposure');
    const node = await openDetails('Exposure');
    const slider = await node.$('input[aria-label="Exposure slider"]');
    const field = await node.$('input[aria-label="Exposure"]');
    for (const next of ['1', '1.2', '1.25']) {
      await browser.execute((element, value) => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, value);
        element.dispatchEvent(new Event('input', { bubbles: true }));
      }, slider, next);
    }
    await expect(field).toHaveValue('1.25');
    await expect(node.$('button[aria-label="Reset Exposure"]')).not.toExist();
    await browser.execute((element) => element.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowRight', bubbles: true })), slider);
    await expect(node.$('button[aria-label="Reset Exposure"]')).toExist();
    await expect(field).toHaveValue('1.25');
  });

  it('disables image drawing until a crop input is connected', async () => {
    await addNode('Crop');
    const node = await openDetails('Crop');
    const draw = await node.$('button*=Draw crop region');
    await expect(draw).toBeDisplayed();
    await expect(draw).toBeDisabled();
    await expect(node).toHaveText(expect.stringContaining('Connect an image input to use these controls'));
    await expect($('[aria-label="Draw image region"]')).not.toExist();
  });
});
