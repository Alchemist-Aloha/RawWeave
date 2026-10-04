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
      if (label !== 'Difference') {
        await browser.waitUntil(() => browser.execute(() => {
          const indicators = [...document.querySelectorAll('.viewer-source')];
          return indicators.length > 0 && indicators.every(indicator => {
            const viewer = indicator.getAttribute('data-viewer');
            const source = document.querySelector(`select[aria-label="Viewer ${viewer} target"]`).selectedOptions[0].textContent.trim();
            return indicator.getAttribute('aria-label') === `Viewer ${viewer} source: ${source}`;
          });
        }));
      }
      if (label === 'Blink') {
        await $('.viewer-source[data-viewer="B"]').waitForExist();
        await browser.saveScreenshot('./logs/comparison-blink.png');
      }
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
    await expect($('.viewer-source[data-viewer="B"]')).toHaveAttribute('aria-label', 'Viewer B source: Image Input · Image');
  });

  it('changes docked A/B orientation and wipes left A / right B', async () => {
    const click = async (label) => browser.execute((element) => element.click(), await $(`button[aria-label="${label}"]`));
    await click('Compare A and B');
    const geometry = () => browser.execute(() => {
      const rect = viewer => {
        const { left, right, top, bottom } = document.querySelector(`.viewer-grid [aria-label="Viewer ${viewer}"]`).getBoundingClientRect();
        return { left, right, top, bottom };
      };
      return { A: rect('A'), B: rect('B') };
    });
    const originalSize = await browser.getWindowSize();
    for (const [width, height] of [[1360, 900], [900, 1200]]) {
      await browser.setWindowSize(width, height);
      await click('Viewer layout: side by side');
      const horizontal = await geometry();
      expect(horizontal.A.right).toBeLessThanOrEqual(horizontal.B.left);
      expect(Math.abs(horizontal.A.top - horizontal.B.top)).toBeLessThan(1);
      await click('Viewer layout: stacked');
      const vertical = await geometry();
      expect(vertical.A.bottom).toBeLessThanOrEqual(vertical.B.top);
      expect(Math.abs(vertical.A.left - vertical.B.left)).toBeLessThan(1);
    }
    await browser.setWindowSize(originalSize.width, originalSize.height);
    await expect($('.viewer-source[data-viewer="A"]')).toBeDisplayed();
    await expect($('.viewer-source[data-viewer="B"]')).toBeDisplayed();
    await browser.saveScreenshot('./logs/comparison-stacked.png');
    await click('Viewer layout: side by side');
    await browser.waitUntil(() => browser.execute(() => !document.querySelector('.viewer-pane__progress')), { timeout: 30000 });
    await browser.saveScreenshot('./logs/comparison-horizontal.png');
    await click('Wipe');
    await browser.execute(() => {
      const range = document.querySelector('input[aria-label="Wipe position"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(range, '25');
      range.dispatchEvent(new Event('input', { bubbles: true }));
    });
    const sides = await browser.execute(() => {
      const stage = document.querySelector('.viewer-pane--comparison-a .viewer-pane__stage').getBoundingClientRect();
      const paneAt = fraction => document.elementFromPoint(stage.left + stage.width * fraction, stage.top + stage.height / 2)?.closest('.viewer-pane')?.getAttribute('aria-label');
      return { left: paneAt(0.125), right: paneAt(0.75) };
    });
    expect(sides).toEqual({ left: 'Viewer A', right: 'Viewer B' });
    await expect($('.viewer-source[data-viewer="A"]')).toBeDisplayed();
    await expect($('.viewer-source[data-viewer="B"]')).toBeDisplayed();
    await browser.saveScreenshot('./logs/comparison-wipe.png');
  });
});
