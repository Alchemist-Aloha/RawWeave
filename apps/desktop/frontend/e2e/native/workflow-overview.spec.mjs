import { $, browser, expect } from '@wdio/globals';

async function overviewGeometry() {
  return browser.execute(() => {
    const canvas = document.querySelector('.react-flow').getBoundingClientRect();
    const panel = document.querySelector('.react-flow__minimap').getBoundingClientRect();
    const svg = document.querySelector('.react-flow__minimap-svg').getBoundingClientRect();
    const mask = document.querySelector('.react-flow__minimap-mask').getAttribute('d');
    const viewport = mask.trim().split(/\s+/).at(-1).match(/h([\d.e+-]+)v([\d.e+-]+)/);
    return {
      canvasRatio: canvas.width / canvas.height,
      overviewRatio: svg.width / svg.height,
      maskRatio: Number(viewport[1]) / Number(viewport[2]),
      contained: svg.right <= panel.right + 1 && svg.bottom <= panel.bottom + 1,
      width: svg.width, height: svg.height,
    };
  });
}

describe('workflow overview geometry', () => {
  it('shows newly created nodes without another graph edit', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await browser.execute((element) => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, 'Invert');
      element.dispatchEvent(new Event('input', { bubbles: true }));
    }, search);
    await browser.execute((element) => element.click(), await $('.node-library__item'));
    await $('[aria-label="Invert node"]').waitForDisplayed();
    await browser.waitUntil(() => browser.execute(() => {
      const nodes = document.querySelectorAll('.react-flow__node');
      const overview = document.querySelectorAll('.react-flow__minimap-node');
      return nodes.length > 0 && nodes.length === overview.length
        && [...overview].every((node) => Number(node.getAttribute('width')) > 0 && Number(node.getAttribute('height')) > 0);
    }), { timeout: 2000 });
  });

  it('matches the canvas aspect ratio and stays contained after dock resizing', async () => {
    await $('.react-flow__minimap-svg').waitForDisplayed();
    for (const action of [null, 'Collapse Nodes panel', 'Expand Nodes panel']) {
      if (action) await $(`[aria-label="${action}"]`).click();
      await browser.waitUntil(async () => {
        const geometry = await overviewGeometry();
        return Math.abs(geometry.overviewRatio - geometry.canvasRatio) < 0.02;
      }, { timeout: 2000 });
      const geometry = await overviewGeometry();
      expect(geometry.contained).toBe(true);
      expect(geometry.maskRatio).toBeCloseTo(geometry.canvasRatio, 2);
      expect(geometry.width).toBeLessThanOrEqual(148);
      expect(geometry.height).toBeLessThanOrEqual(100);
    }
    await browser.saveScreenshot('./logs/workflow-overview.png');
  });
});
