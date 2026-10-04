import { $, browser, expect } from '@wdio/globals';

const invoke = (command) => browser.tauri.execute((tauri, name) => tauri.core.invoke(name), command);
const draft = (field, text) => browser.execute((element, next) => {
  const prototype = element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, next);
  element.dispatchEvent(new Event('input', { bubbles: true }));
}, field, text);

describe('native transfer curve display', () => {
  for (const [typeId, name, parameter, next, expectedText] of [
    ['core.levels', 'Levels', 'Midtone Gamma', '2', 'Black 0 maps to 0; white 1 maps to 1'],
    ['core.map-range', 'Map Range', 'Output Maximum', '-1', 'Input 0 maps to 0; input 1 maps to −1'],
    ['core.clamp', 'Clamp', 'Maximum', '0.5', 'values above 0.5 hold at 0.5'],
  ]) {
    it(`displays committed ${name} parameters without creating draft commands`, async () => {
      await browser.setWindowSize(1600, 1000);
      const search = await $('input[placeholder="Search nodes"]');
      await draft(search, typeId);
      await browser.execute((element) => element.click(), await $('.node-library__item'));
      const node = await $(`[aria-label="${name} node"]`);
      await node.waitForExist();
      await browser.execute((element) => { element.querySelector('details').open = true; element.querySelector('.graph-node__title').click(); }, node);
      const field = await node.$(`input[aria-label="${parameter}"]`);
      const diagram = await node.$('.parameter-transfer-preview');
      await expect(diagram.$('svg')).toBeDisplayed();
      const initial = await diagram.$('polyline').getAttribute('points');
      const before = await invoke('workflow_hash');
      await draft(field, next);
      expect(await invoke('workflow_hash')).toBe(before);
      expect(await diagram.$('polyline').getAttribute('points')).toBe(initial);
      await browser.execute((element) => {
        element.focus();
        element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
        element.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
      }, field);
      await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
      await expect(diagram).toHaveText(expect.stringContaining(expectedText));
      expect(await diagram.$('polyline').getAttribute('points')).not.toBe(initial);
      await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
      await browser.waitUntil(async () => (await invoke('workflow_hash')) === before);
      expect(await diagram.$('polyline').getAttribute('points')).toBe(initial);
      const edit = async (label, text) => {
        const input = await node.$(`input[aria-label="${label}"]`);
        await draft(input, text);
        await browser.execute((element) => {
          element.focus();
          element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
          element.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
        }, input);
        await expect(input).toHaveValue(text);
      };
      if (typeId === 'core.map-range') {
        await edit('Input Minimum', '1');
        await expect(diagram.$('svg')).not.toExist();
        await edit('Input Maximum', '0');
        await expect(diagram).toHaveText(expect.stringContaining('Input 1 maps to 0; input 0 maps to 1'));
        const extrapolated = await diagram.$('polyline').getAttribute('points');
        await browser.execute((element) => element.click(), await node.$('input[type="checkbox"]'));
        await expect(diagram).toHaveText(expect.stringContaining('Clamped'));
        expect(await diagram.$('polyline').getAttribute('points')).not.toBe(extrapolated);
      } else if (typeId === 'core.clamp') {
        await edit('Minimum', '2');
        await expect(diagram).toHaveText(expect.stringContaining('Minimum must not exceed Maximum'));
        await expect(diagram.$('svg')).not.toExist();
        await edit('Maximum', '2');
        await expect(diagram.$('svg')).toBeDisplayed();
      } else {
        await edit('White Point', '0');
        await expect(diagram).toHaveText(expect.stringContaining('White Point must be greater than Black Point'));
        await expect(diagram.$('svg')).not.toExist();
        await edit('White Point', '1.5');
        await edit('Midtone Gamma', '2');
        await expect(diagram).toHaveText(expect.stringContaining('white 1.5 maps to 1'));
      }
      await $('.react-flow__controls-fitview').click();
      await browser.pause(300);
      await browser.saveScreenshot(`./logs/${typeId}-transfer-native.png`);
      await $('.lamp').click();
      await browser.pause(200);
      await browser.saveScreenshot(`./logs/${typeId}-transfer-lit-native.png`);
      await $('.lamp').click();
      await browser.execute((element) => element.click(), await node.$(`[aria-label="Delete ${name}"]`));
      await draft(search, '');
    });
  }
  it('plots scalar points, keeps drafts local, and saves one undoable edit', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await draft(search, 'core.curve');
    await browser.execute(() => [...document.querySelectorAll('.node-library__item')].find((item) => item.textContent.includes('core.curve') && !item.textContent.includes('core.curves')).click());
    const node = await $('[aria-label="Curve node"]');
    await node.waitForExist();
    await browser.execute((element) => { element.querySelector('details').open = true; }, node);
    const field = await node.$('textarea[aria-label="Curve Points"]');
    await expect(field).toBeDisplayed();
    await expect(node.$('.curve-preview svg')).toBeDisplayed();
    const before = await invoke('workflow_hash');
    const initial = await node.$('.curve-preview polyline').getAttribute('points');
    await draft(field, '0,0;0.5,0.7;1,1');
    expect(await invoke('workflow_hash')).toBe(before);
    expect(await node.$('.curve-preview polyline').getAttribute('points')).not.toBe(initial);
    await browser.execute((element) => {
      element.focus();
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      element.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    }, field);
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
    const saved = JSON.parse(await invoke('save_workflow'));
    expect(Object.values(saved.nodes).some((value) => value.parameters.points?.String === '0,0;0.5,0.7;1,1')).toBe(true);
    await $('.react-flow__controls-fitview').click();
    await browser.pause(300);
    await browser.saveScreenshot('./logs/curve-preview-dark-native.png');
    await $('.lamp').click();
    await browser.pause(200);
    await browser.saveScreenshot('./logs/curve-preview-lit-native.png');
    await $('.lamp').click();
    await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
    await browser.waitUntil(async () => (await invoke('workflow_hash')) === before);
    await expect(field).toHaveValue('0,0;1,1');
    await draft(search, '');
  });
});
