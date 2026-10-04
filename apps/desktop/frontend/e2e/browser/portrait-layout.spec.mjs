import { $, browser, expect } from '@wdio/globals';

describe('adaptive workbench', () => {
  it('stacks in portrait and restores landscape with the library still left', async () => {
    await browser.url('/');
    await $('.canvas-panel').waitForDisplayed();
    for (const [width, height] of [[1440, 900], [900, 1200], [1440, 900], [600, 1000], [960, 640]]) {
      await browser.setWindowSize(width, height);
      const portrait = height >= width;
      await expect($('[aria-label="Resize right panel"]')).toHaveAttribute('aria-orientation', portrait ? 'horizontal' : 'vertical');
      const boxes = await browser.execute(() => {
        const rect = (selector) => {
          const { left, right, top, bottom, width } = document.querySelector(selector).getBoundingClientRect();
          return { left, right, top, bottom, width };
        };
        return { library: rect('.dock--left'), graph: rect('.canvas-panel'), info: rect('.dock--right'), overflow: document.documentElement.scrollWidth > innerWidth };
      });
      expect(boxes.library.right).toBeLessThanOrEqual(boxes.graph.left);
      expect(boxes.overflow).toBe(false);
      if (portrait) {
        expect(boxes.graph.right).toBeLessThanOrEqual(width);
        expect(boxes.graph.width).toBeGreaterThan(0);
      } else {
        expect(boxes.graph.width).toBeGreaterThanOrEqual(480);
      }
      const widgets = await browser.execute(() => {
        const rect = selector => { const { left, right } = document.querySelector(selector).getBoundingClientRect(); return { left, right }; };
        return { controls: rect('.react-flow__controls'), overview: rect('.react-flow__minimap') };
      });
      expect(widgets.controls.left).toBeGreaterThanOrEqual(boxes.graph.left);
      expect(widgets.controls.right).toBeLessThanOrEqual(widgets.overview.left);
      expect(widgets.overview.right).toBeLessThanOrEqual(boxes.graph.right);
      await browser.waitUntil(() => browser.execute(() => {
        const canvas = document.querySelector('.react-flow').getBoundingClientRect();
        const panel = document.querySelector('.react-flow__minimap').getBoundingClientRect();
        const svg = document.querySelector('.react-flow__minimap-svg').getBoundingClientRect();
        return Math.abs(svg.width / svg.height - canvas.width / canvas.height) < 0.02
          && svg.right <= panel.right + 1 && svg.bottom <= panel.bottom + 1;
      }));
      if (portrait) {
        expect(boxes.info.top).toBeGreaterThanOrEqual(boxes.graph.bottom);
        expect(boxes.info.left).toBe(boxes.graph.left);
        expect(boxes.info.width).toBe(boxes.graph.width);
      } else {
        expect(boxes.info.left).toBeGreaterThanOrEqual(boxes.graph.right);
      }
    }
  });
});
