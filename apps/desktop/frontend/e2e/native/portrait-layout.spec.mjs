import { $, browser, expect } from '@wdio/globals';

async function geometry() {
  return browser.execute(() => {
    const rect = (selector) => {
      const { left, top, right, bottom, width, height } = document.querySelector(selector).getBoundingClientRect();
      return { left, top, right, bottom, width, height };
    };
    return {
      portrait: matchMedia('(orientation: portrait)').matches,
      library: rect('.dock--left'), graph: rect('.canvas-panel'), info: rect('.dock--right'),
    };
  });
}

const workflowHash = () => browser.tauri.execute(({ core }) => core.invoke('workflow_hash'));

describe('portrait workbench layout', () => {
  it('keeps the library left and graph above preview/info in portrait without changing the graph', async () => {
    await $('.canvas-panel').waitForDisplayed();
    const hash = await workflowHash();
    await browser.setWindowSize(900, 1200);
    await browser.waitUntil(async () => (await geometry()).portrait);
    const portrait = await geometry();
    expect(portrait.library.right).toBeLessThanOrEqual(portrait.graph.left);
    expect(portrait.info.top).toBeGreaterThanOrEqual(portrait.graph.bottom);
    expect(portrait.info.left).toBe(portrait.graph.left);
    expect(portrait.info.width).toBe(portrait.graph.width);
    expect(portrait.graph.height).toBeGreaterThan(180);
    await expect($('.viewer-pane__image')).toBeDisplayed();
    const divider = await $('[aria-label="Resize right panel"]');
    await expect(divider).toHaveAttribute('aria-orientation', 'horizontal');
    await browser.execute((element) => element.focus(), divider);
    await browser.execute((element) => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true, cancelable: true })), divider);
    await browser.waitUntil(async () => (await geometry()).info.height > portrait.info.height);
    await expect($('[aria-label="Resize preview panel"]')).toHaveAttribute('aria-orientation', 'vertical');
    await browser.saveScreenshot('./logs/portrait-workbench.png');
    expect(await workflowHash()).toBe(hash);
  });

  it('collapses and expands portrait panels without losing the workspace', async () => {
    await browser.setWindowSize(900, 1200);
    await browser.waitUntil(async () => (await geometry()).portrait);
    await $('[aria-label="Collapse source panel"]').click();
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.dock-section--source').getBoundingClientRect().width === 34));
    await expect($('.viewer-pane__image')).toBeDisplayed();
    await $('[aria-label="Expand source panel"]').click();
    await $('[aria-label="Collapse preview panel"]').click();
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.dock-section--preview').getBoundingClientRect().width === 34));
    await expect($('.source-metadata')).toBeDisplayed();
    await $('[aria-label="Expand preview panel"]').click();
    await $('[aria-label="Collapse Nodes panel"]').click();
    await browser.waitUntil(async () => (await geometry()).library.width === 32);
    expect((await geometry()).info.left).toBe((await geometry()).graph.left);
    await $('[aria-label="Expand Nodes panel"]').click();
    await expect($('.viewer-pane__image')).toBeDisplayed();
  });
});
