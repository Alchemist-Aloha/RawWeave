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
    const slider = await node.$('input[aria-label="Radius slider"]');
    await expect(slider).toBeDisplayed();
    await expect(slider).toHaveAttribute('min', '0');
    await expect(slider).toHaveAttribute('max', '64');
    await browser.execute((element, next) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(element, next);
      element.dispatchEvent(new Event('input', { bubbles: true }));
      element.dispatchEvent(new Event('change', { bubbles: true }));
    }, slider, '32');
    await expect(node.$('input[aria-label="Radius"]')).toHaveValue('32');
    const reset = await node.$('button[aria-label="Reset Radius"]');
    await browser.execute((element) => element.click(), reset);
    await expect(node.$('input[aria-label="Radius"]')).toHaveValue('1');
    await expect(slider).toHaveValue('1');
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
