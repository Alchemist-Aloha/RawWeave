import { $, browser, expect } from '@wdio/globals';

async function targets() {
  return browser.execute(() => Object.fromEntries(['A', 'B'].map((viewer) => {
    const select = document.querySelector(`select[aria-label="Viewer ${viewer} target"]`);
    return [viewer, select?.selectedOptions[0]?.textContent.trim()];
  })));
}

describe('comparison preview defaults', () => {
  it('compares the image input with final output in every comparison mode', async () => {
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 30000 });
    for (const label of ['Compare A and B', 'Wipe', 'Blink', 'Difference']) {
      await browser.execute((element) => element.click(), await $(`button[aria-label="${label}"]`));
      await browser.waitUntil(async () => {
        const selected = await targets();
        return selected.A === 'Image Input · Image' && selected.B === 'Output · Image';
      });
    }
    await browser.waitUntil(() => browser.execute(() => {
      const images = [...document.querySelectorAll('.viewer-pane__image')];
      return images.length === 2 && images.every((image) => image.complete && image.naturalWidth > 0);
    }), { timeout: 30000 });
    await browser.saveScreenshot('./logs/comparison-defaults.png');
    // Explicit targets survive a comparison-mode change.
    const a = await $('select[aria-label="Viewer A target"]').getValue();
    await browser.execute((value) => {
      const select = document.querySelector('select[aria-label="Viewer B target"]');
      select.value = value;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    }, a);
    await browser.execute((element) => element.click(), await $('button[aria-label="Wipe"]'));
    expect((await targets()).B).toBe('Image Input · Image');
  });
});
