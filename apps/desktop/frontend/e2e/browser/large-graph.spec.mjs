import { $, browser, expect } from '@wdio/globals';

// A realistic overview needs zoom below React Flow's default 50% floor.
describe('large workflow overview', () => {
  it('fits all 200 nodes without clipping the outer columns', async () => {
    await browser.url('/');
    await $('.node-library__item').waitForExist();
    await browser.execute(() => {
      const nodes = Array.from({ length: 200 }, (_, index) => ({
        id: `exposure-${index}`, typeId: 'core.exposure', parameters: { exposure: 0 }, exposedParameters: [],
      }));
      const positions = Object.fromEntries(nodes.map((node, index) => [node.id, {
        x: (index % 20) * 300, y: Math.floor(index / 20) * 150,
      }]));
      const files = new DataTransfer();
      files.items.add(new File([JSON.stringify({
        version: 1, graph: JSON.stringify({ nodes, edges: [], revision: 0 }), positions,
      })], 'large-workflow.json', { type: 'application/json' }));
      const input = document.querySelectorAll('.topbar__actions input[type="file"]')[2];
      input.files = files.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('200 nodes'));
    for (const width of [1440, 1100]) {
      await browser.setWindowSize(width, 900);
      await $('button[aria-label="Fit View"]').click();
      await browser.waitUntil(() => browser.execute(() => {
        const canvas = document.querySelector('.flow-canvas').getBoundingClientRect();
        const nodes = [...document.querySelectorAll('.react-flow__node')];
        return nodes.length === 200 && nodes.every((node) => {
          const rect = node.getBoundingClientRect();
          return rect.left >= canvas.left && rect.right <= canvas.right
            && rect.top >= canvas.top && rect.bottom <= canvas.bottom;
        });
      }), { timeoutMsg: `Fit view clipped a large workflow at ${width}px` });
    }
  });
});
