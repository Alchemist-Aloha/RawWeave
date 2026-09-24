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

  it('keeps the workflow graph visible after switching workspaces', async () => {
    const workspace = await $('section.workspace');
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
