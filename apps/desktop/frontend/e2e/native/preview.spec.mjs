import { $, $$, browser, expect } from '@wdio/globals';

async function selectMode(label) {
  for (const button of await $$('nav[aria-label="Workspace mode"] button')) {
    if (await button.getText() === label) {
      await button.click();
      return;
    }
  }
  throw new Error(`workspace mode ${label} was not found`);
}

describe('real Tauri preview', () => {
  it('reattaches a saved image and displays the rendered PNG', async () => {
    const descriptorCount = await browser.tauri.execute(({ core }) =>
      core.invoke('node_descriptors').then((descriptors) => descriptors.length));
    expect(descriptorCount).toBeGreaterThan(0);
    await expect($('select[aria-label="Viewer A target"]')).toBeDisplayed();
    await $('select[aria-label="Viewer A target"] option:nth-child(2)').waitForExist({
      timeout: 15_000,
      timeoutMsg: 'restoring the saved image did not add a preview output',
    });

    const image = await $('.viewer-pane__image');
    await image.waitForDisplayed();
    await browser.waitUntil(async () => browser.execute((element) => element.complete && element.naturalWidth > 0, image), {
      timeout: 15_000,
      timeoutMsg: 'rendered Tauri preview PNG did not load',
    });
    await expect(image).toHaveAttribute('src', expect.stringContaining('rawweave-preview'));
    await expect($('section[aria-label="Image viewers"] [role="alert"]')).not.toExist();
    await expect($('section[aria-label="Image scopes"]')).toBeDisplayed();
  });

  it('keeps a rendered preview on screen while the graph and layout change', async () => {
    const image = await $('.viewer-pane__image');
    await image.waitForDisplayed({ timeout: 20_000 });
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.viewer-pane__image')?.complete === true), {
      timeout: 20_000,
      timeoutMsg: 'the restored preview did not finish loading',
    });

    await browser.execute(() => {
      window.__flicker = { samples: 0, missing: 0 };
      window.__flickerTimer = window.setInterval(() => {
        window.__flicker.samples += 1;
        const element = document.querySelector('.viewer-pane__image');
        if (!element || !element.complete || element.naturalWidth === 0) window.__flicker.missing += 1;
      }, 25);
    });

    // Adding a node changes the canvas height, and dragging one publishes
    // editor state; both used to restart the preview and blank the pane.
    const search = await $('input[placeholder="Search nodes"]');
    await search.setValue('Exposure');
    const item = await $('.node-library__item');
    await item.waitForDisplayed();
    await item.click();
    await browser.pause(800);
    await browser.execute(async () => {
      const node = document.querySelector('.react-flow__node');
      if (!node) return;
      const rect = node.getBoundingClientRect();
      const x = rect.left + rect.width / 2;
      const y = rect.top + 12;
      const fire = (target, type, at) => target.dispatchEvent(new MouseEvent(type, {
        bubbles: true, cancelable: true, composed: true, view: window,
        detail: 1, button: 0, buttons: type === 'mouseup' ? 0 : 1, clientX: at.x, clientY: at.y,
      }));
      fire(node, 'mousedown', { x, y });
      for (let step = 1; step <= 10; step += 1) {
        fire(window, 'mousemove', { x: x + step * 6, y: y + step * 4 });
        await new Promise((resolve) => setTimeout(resolve, 40));
      }
      fire(window, 'mouseup', { x: x + 60, y: y + 40 });
    });
    await browser.pause(2500);

    const flicker = await browser.execute(() => {
      window.clearInterval(window.__flickerTimer);
      return window.__flicker;
    });
    expect(flicker.samples).toBeGreaterThan(20);
    expect(flicker.missing).toBe(0);
    await expect($('.viewer-pane__image')).toBeDisplayed();

    // Restore the shared graph for the specs that run after this one.
    await $('[aria-label="Exposure node"]').click();
    await $('[aria-label="Delete Exposure"]').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));
  });

  it('lays the preview out without dead space', async () => {
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
    await browser.pause(1500);

    const layout = await browser.execute(() => {
      const section = document.querySelector('.viewer-section');
      const rows = getComputedStyle(section).gridTemplateRows.split(' ').map(Number.parseFloat);
      return {
        section: Math.round(section.getBoundingClientRect().height),
        rows,
        toolbar: Math.round(document.querySelector('.viewer-section__toolbar')?.getBoundingClientRect().height ?? 0),
        stage: Math.round(document.querySelector('.viewer-pane__stage')?.getBoundingClientRect().height ?? 0),
        scopes: Math.round(document.querySelector('.viewer-scopes')?.getBoundingClientRect().height ?? 0),
        tabs: document.querySelectorAll('.scope-tab').length,
      };
    });

    // One compact toolbar row, a tabbed scope strip, and no unused track.
    expect(layout.toolbar).toBeLessThan(60);
    expect(layout.tabs).toBe(8);
    expect(layout.stage).toBeGreaterThan(120);
    expect(layout.rows.reduce((total, value) => total + value, 0)).toBeGreaterThan(layout.section - 14);
  });

  it('keeps the workflow graph visible after switching workspaces', async () => {
    const workspace = await $('.workbench');
    const flow = await $('.react-flow');
    await flow.waitForDisplayed();
    await $('.react-flow__node').waitForDisplayed();
    await $('.react-flow__edge-path').waitForExist();
    const handleOffsets = await browser.execute(() => [...document.querySelectorAll('.graph-node__port')]
      .map((port) => {
        const handle = port.querySelector('.react-flow__handle');
        if (!handle) return null;
        const row = port.getBoundingClientRect();
        const point = handle.getBoundingClientRect();
        return Math.abs((point.top + point.height / 2) - (row.top + row.height / 2));
      })
      .filter((offset) => offset !== null));
    expect(handleOffsets.length).toBeGreaterThan(0);
    for (const offset of handleOffsets) expect(offset).toBeLessThan(3);

    for (const mode of ['Browse / Queue', 'Batch', 'Integrations']) {
      await selectMode(mode);
      await selectMode('Build / Preview');
      await expect(workspace).toBeDisplayed();
      await expect(flow).toBeDisplayed();
      await expect($('.react-flow__node')).toBeDisplayed();
      const geometry = await browser.execute(() => {
        const canvas = document.querySelector('.flow-canvas')?.getBoundingClientRect();
        const node = document.querySelector('.react-flow__node')?.getBoundingClientRect();
        return { canvasWidth: canvas?.width ?? 0, canvasHeight: canvas?.height ?? 0, nodeWidth: node?.width ?? 0 };
      });
      expect(geometry.canvasWidth).toBeGreaterThan(100);
      expect(geometry.canvasHeight).toBeGreaterThan(100);
      expect(geometry.nodeWidth).toBeGreaterThan(100);
    }
  });
});
