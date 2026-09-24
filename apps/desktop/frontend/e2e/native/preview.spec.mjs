import { $, browser, expect } from '@wdio/globals';

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
});
