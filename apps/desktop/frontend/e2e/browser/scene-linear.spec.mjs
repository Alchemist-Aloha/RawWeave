import { $, browser, expect } from '@wdio/globals';

describe('scene-linear node discovery', () => {
  it('exposes typed scene sockets and the explicit image conversion node', async () => {
    await browser.url('http://127.0.0.1:5178/');
    await $('.react-flow').waitForDisplayed();
    const search = await $('input[placeholder="Search nodes"]');
    await search.setValue('core.exposure');
    await $('.node-library__item').click();
    const exposure = await $('.react-flow__node[data-id="exposure"]');
    await exposure.waitForDisplayed();
    await expect(exposure.$('.react-flow__handle.source[data-handleid="scene"]')).toBeExisting();
    await expect(exposure.$('.react-flow__handle.target[data-handleid="scene"]')).toBeExisting();
    await expect(exposure.$('.react-flow__handle.source[data-handleid="image"]')).toBeExisting();
    await search.setValue('core.scene-linear-to-image');
    await expect($('.node-library__item')).toHaveText(expect.stringContaining('Scene Linear RGB to Image'));
    await $('.node-library__item').click();
    const conversion = await $('.react-flow__node[data-id="scene-linear-to-image"]');
    await conversion.waitForDisplayed();
    await expect(conversion.$('.react-flow__handle.target[data-handleid="scene"]')).toBeExisting();
    await expect(conversion.$('.react-flow__handle.source[data-handleid="image"]')).toBeExisting();
  });
});
