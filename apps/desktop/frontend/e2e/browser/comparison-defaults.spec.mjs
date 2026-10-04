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
  });
});
