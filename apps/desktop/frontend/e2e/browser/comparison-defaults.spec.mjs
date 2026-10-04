import { $, $$, browser, expect } from '@wdio/globals';
import { fileURLToPath } from 'node:url';

const imageFixture = fileURLToPath(new URL('../../../../../test-data/images/common/pngsuite-rgb8.png', import.meta.url));

describe('comparison preview defaults', () => {
  it('uses image input and final output for all comparison surfaces', async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => localStorage.clear());
    await browser.refresh();
    const inputs = await $$('.topbar__actions input[type="file"]');
    await inputs[1].addValue(await browser.uploadFile(imageFixture));
    await $('.viewer-pane__image').waitForDisplayed();
    for (const label of ['Compare A and B', 'Wipe', 'Blink', 'Difference']) {
      await $(`button[aria-label="${label}"]`).click();
      await browser.waitUntil(() => browser.execute(() => {
        const selected = viewer => document.querySelector(`select[aria-label="Viewer ${viewer} target"]`)?.selectedOptions[0]?.textContent.trim();
        return selected('A') === 'Image Input · Image' && selected('B') === 'Output · Image';
      }));
    }
    const a = await $('select[aria-label="Viewer A target"]').getValue();
    await $('select[aria-label="Viewer B target"]').selectByAttribute('value', a);
    await $('button[aria-label="Wipe"]').click();
    await expect($('select[aria-label="Viewer B target"]')).toHaveValue(a);
    await expect($('.viewer-source[data-viewer="B"]')).toHaveAttribute('aria-label', 'Viewer B source: Image Input · Image');
  });

  it('changes docked A/B orientation and shows A left / B right in wipe', async () => {
    await $('button[aria-label="Compare A and B"]').click();
    const geometry = () => browser.execute(() => ['A', 'B'].map(viewer => {
      const { left, right, top, bottom } = document.querySelector(`.viewer-grid [aria-label="Viewer ${viewer}"]`).getBoundingClientRect();
      return { left, right, top, bottom };
    }));
    await $('button[aria-label="Viewer layout: side by side"]').click();
    const [a, b] = await geometry();
    expect(a.right).toBeLessThanOrEqual(b.left);
    expect(Math.abs(a.top - b.top)).toBeLessThan(1);
    await $('button[aria-label="Viewer layout: stacked"]').click();
    const [top, bottom] = await geometry();
    expect(top.bottom).toBeLessThanOrEqual(bottom.top);
    await expect($('.viewer-source[data-viewer="A"]')).toBeDisplayed();
    await expect($('.viewer-source[data-viewer="B"]')).toBeDisplayed();
    await $('button[aria-label="Wipe"]').click();
    await browser.execute(() => {
      const range = document.querySelector('input[aria-label="Wipe position"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(range, '25');
      range.dispatchEvent(new Event('input', { bubbles: true }));
    });
    expect(await browser.execute(() => {
      const rect = document.querySelector('.viewer-pane--comparison-a .viewer-pane__stage').getBoundingClientRect();
      return [0.125, 0.75].map(fraction => document.elementFromPoint(rect.left + rect.width * fraction, rect.top + rect.height / 2)?.closest('.viewer-pane')?.getAttribute('aria-label'));
    })).toEqual(['Viewer A', 'Viewer B']);
    await $('button[aria-label="Blink"]').click();
    await $('.viewer-source[data-viewer="B"]').waitForExist();
    await expect($('.viewer-source')).toHaveAttribute('aria-label', 'Viewer B source: Image Input · Image');
  });
});
