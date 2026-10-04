import { $, browser, expect } from '@wdio/globals';

const draft = (field, text) => browser.execute((element, next) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, next);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}, field, text);

describe('color parameter references', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => localStorage.clear());
    await browser.refresh();
    await $('input[placeholder="Search nodes"]').waitForDisplayed();
  });
  for (const [typeId, name, label, next, expected] of [
    ['core.mask-color-qualifier', 'Color Qualifier', 'Target Red', '0.25', 'RGB 0.25, 1, 1'],
    ['pro.color-zones', 'Color Zones', 'Target Hue', '350', 'Target Hue 350°'],
    ['pro.split-toning', 'Split Toning', 'Shadow Hue', '300', 'Shadow Hue 300°'],
  ]) {
    it(`retains keyboard editing and Undo for ${name}`, async () => {
      await draft(await $('input[placeholder="Search nodes"]'), typeId);
      await $('.node-library__item').click();
      const node = await $(`[aria-label="${name} node"]`);
      await node.$('summary').click();
      const preview = await node.$('.color-parameter-preview');
      await expect(preview).toBeDisplayed();
      const initial = await preview.getText();
      const field = await node.$(`input[aria-label="${label}"]`);
      await draft(field, next);
      expect(await preview.getText()).toBe(initial);
      await browser.execute((element) => element.focus(), field);
      await browser.keys('Escape');
      expect(await preview.getText()).toBe(initial);
      await draft(field, next);
      await browser.keys('Enter');
      await expect(preview).toHaveText(expect.stringContaining(expected));
      if (typeId === 'pro.color-zones') expect((await preview.$$('.color-hue__interval')).length).toBe(2);
      await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
      await expect(preview).toHaveText(initial);
    });
  }
});
