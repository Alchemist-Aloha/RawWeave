import { $, browser, expect } from '@wdio/globals';

const invoke = (command) => browser.tauri.execute((tauri, name) => tauri.core.invoke(name), command);
const draft = (field, text) => browser.execute((element, next) => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, next);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}, field, text);
const commit = (field) => browser.execute((element) => {
  element.focus();
  element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  element.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
}, field);

describe('native color parameter references', () => {
  for (const [typeId, name, label, next, parameterId, savedValue, expected] of [
    ['core.mask-color-qualifier', 'Color Qualifier', 'Target Red', '0.25', 'target_r', 0.25, 'RGB 0.25, 1, 1'],
    ['pro.color-zones', 'Color Zones', 'Target Hue', '350', 'hue', 350 / 360, 'Target Hue 350°'],
    ['pro.split-toning', 'Split Toning', 'Shadow Hue', '300', 'shadow_hue', 300 / 360, 'Shadow Hue 300°'],
  ]) {
    it(`keeps ${name} exact fields and updates its reference once on commit`, async () => {
      const search = await $('input[placeholder="Search nodes"]');
      await draft(search, typeId);
      await browser.execute((element) => element.click(), await $('.node-library__item'));
      const node = await $(`[aria-label="${name} node"]`);
      await node.waitForExist();
      await browser.execute((element) => { element.querySelector('details').open = true; element.querySelector('.graph-node__title').click(); }, node);
      const preview = await node.$('.color-parameter-preview');
      await expect(preview).toBeDisplayed();
      const initial = await preview.getText();
      const before = await invoke('workflow_hash');
      const field = await node.$(`input[aria-label="${label}"]`);
      await draft(field, next);
      expect(await invoke('workflow_hash')).toBe(before);
      expect(await preview.getText()).toBe(initial);
      await commit(field);
      await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
      await expect(preview).toHaveText(expect.stringContaining(expected));
      const saved = JSON.parse(await invoke('save_workflow'));
      const persisted = Object.values(saved.nodes).find((value) => value.type_id === typeId);
      expect(persisted.parameters[parameterId].Float).toBeCloseTo(savedValue, 6);
      if (typeId === 'core.mask-color-qualifier') {
        await expect(preview).toHaveText(expect.stringContaining('not color-managed'));
        expect(await browser.execute((element) => getComputedStyle(element).backgroundColor, await preview.$('.color-reference__swatch'))).toBe('rgb(64, 255, 255)');
      } else {
        const markers = await preview.$$('.color-hue__marker');
        expect(markers.length).toBe(typeId === 'pro.split-toning' ? 2 : 1);
        if (typeId === 'pro.color-zones') expect((await preview.$$('.color-hue__interval')).length).toBe(2);
        else await expect(preview).toHaveText(expect.stringContaining('No tint at 0% saturation'));
      }
      await $('.react-flow__controls-fitview').click();
      await browser.pause(300);
      await browser.saveScreenshot(`./logs/${typeId}-color-native.png`);
      await $('.lamp').click();
      await browser.pause(200);
      await browser.saveScreenshot(`./logs/${typeId}-color-lit-native.png`);
      await $('.lamp').click();
      await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
      await browser.waitUntil(async () => (await invoke('workflow_hash')) === before);
      await expect(preview).toHaveText(initial);
      await browser.execute((element) => element.click(), await node.$(`[aria-label="Delete ${name}"]`));
      await draft(search, '');
    });
  }
});
