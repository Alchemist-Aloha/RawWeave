import { $, browser, expect } from '@wdio/globals';

const invoke = (command) => browser.tauri.execute((tauri, name) => tauri.core.invoke(name), command);
async function add(typeId, name) {
  const search = await $('input[placeholder="Search nodes"]');
  await browser.execute((field, next) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, next);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }, search, typeId);
  await browser.execute((type) => [...document.querySelectorAll('.node-library__item')].find((item) => item.querySelector('small')?.textContent === type).click(), typeId);
  const node = await $(`[aria-label="${name} node"]`);
  await node.waitForExist();
  await browser.execute((element) => { element.querySelector('details').open = true; element.querySelector('.graph-node__title').click(); }, node);
  await $('.react-flow__controls-fitview').click();
  await browser.pause(300);
  return node;
}

async function remove(node, name) {
  await browser.execute((element) => element.click(), await node.$(`[aria-label="Delete ${name}"]`));
  await browser.execute(() => {
    const field = document.querySelector('input[placeholder="Search nodes"]');
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, '');
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
}

async function pointer(svg, type, x, y, altKey = false) {
  await browser.execute((element, kind, a, b, alt) => {
    const m = element.getScreenCTM();
    element.dispatchEvent(new PointerEvent(kind, { bubbles: true, cancelable: true, pointerId: 7, pointerType: 'mouse', button: 0, altKey: alt,
      buttons: kind === 'pointerup' ? 0 : 1, clientX: m.a * a + m.c * b + m.e, clientY: m.b * a + m.d * b + m.f }));
  }, svg, type, x, y, altKey);
}

async function undo(hash) {
  await browser.execute(() => [...document.querySelectorAll('button')].find((button) => button.textContent.trim() === 'Undo').click());
  await browser.waitUntil(async () => (await invoke('workflow_hash')) === hash);
}

describe('direct curve gestures in the native command bridge', () => {
  for (const [typeId, name] of [['core.curve', 'Curve'], ['pro.lut', 'LUT'], ['pro.lut-tools', 'LUT Tools'], ['pro.film-curve', 'Film Curve'], ['pro.film-simulation', 'Film Simulation']]) {
    it(`click-drags ${name} locally, cancels safely, and commits one undoable point edit`, async () => {
      const node = await add(typeId, name);
      const svg = await node.$('.curve-editor svg');
      // Embedded WebDriver cannot deliver physical pointer actions. Synthetic
      // pointers have no native capture identity: emulate only capture bookkeeping
      // while testing actual WebKit SVG matrices and the real Rust command bridge.
      await browser.execute((element) => {
        let captured = null;
        element.setPointerCapture = (id) => { captured = id; };
        element.hasPointerCapture = (id) => captured === id;
        element.releasePointerCapture = () => { captured = null; };
      }, svg);
      const before = await invoke('workflow_hash');
      const initial = await node.$('textarea').getValue();
      const count = (await node.$$('.curve-editor__point')).length;
      await pointer(svg, 'pointerdown', 100, 60);
      await pointer(svg, 'pointermove', 100, 49.6);
      await pointer(svg, 'pointermove', 100, 39.2);
      expect(await invoke('workflow_hash')).toBe(before);
      expect((await node.$$('.curve-editor__point')).length).toBe(count + 1);
      await pointer(svg, 'pointerup', 100, 39.2);
      await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
      const committed = await invoke('workflow_hash');
      const text = await node.$('textarea').getValue();
      const point = text.split(';').map((pair) => pair.split(',').map(Number)).find(([x]) => Math.abs(x - 0.5) < 0.00001);
      expect(point[1]).toBeCloseTo(0.7, 5);
      const saved = JSON.parse(await invoke('save_workflow'));
      expect(Object.values(saved.nodes).some((value) => value.type_id === typeId && value.parameters.points?.String === text)).toBe(true);
      await pointer(svg, 'pointerdown', 100, 39.2);
      await pointer(svg, 'pointermove', 120, 60);
      await browser.execute((element) => element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })), svg);
      await pointer(svg, 'pointerup', 120, 60);
      expect(await invoke('workflow_hash')).toBe(committed);
      await expect(node.$('textarea')).toHaveValue(text);
      await browser.saveScreenshot(`./logs/${typeId}-drag-native.png`);
      await undo(before);
      await expect(node.$('textarea')).toHaveValue(initial);
      await remove(node, name);
    });
  }
  it('edits precise HDR coordinates locally and deletes points with one Undo each', async () => {
    const node = await add('core.curve', 'Curve');
    const original = await invoke('workflow_hash');
    await browser.execute((element) => element.click(), await node.$('[aria-label="Add curve point"]'));
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== original);
    const baseline = await invoke('workflow_hash');
    const output = await node.$('[aria-label="Point output"]');
    await browser.execute((field) => {
      field.focus();
      for (const value of ['1.5', '2.5']) {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, value);
        field.dispatchEvent(new Event('input', { bubbles: true }));
      }
    }, output);
    await expect(node.$('textarea')).toHaveValue('0,0;0.5,2.5;1,1');
    expect(await invoke('workflow_hash')).toBe(baseline);
    await browser.execute((field) => field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true })), output);
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== baseline);
    const saved = JSON.parse(await invoke('save_workflow'));
    expect(Object.values(saved.nodes).some((value) => value.type_id === 'core.curve' && value.parameters.points?.String === '0,0;0.5,2.5;1,1')).toBe(true);
    await browser.saveScreenshot('./logs/curve-coordinates-native.png');
    await undo(baseline);
    await expect(node.$('textarea')).toHaveValue('0,0;0.5,0.5;1,1');
    await browser.execute((element) => element.click(), await node.$('[aria-label="Delete curve point"]'));
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== baseline);
    await expect(node.$('textarea')).toHaveValue('0,0;1,1');
    await undo(baseline);
    const picker = await node.$('[aria-label="Selected curve point"]');
    await browser.execute((field) => { field.value = '0'; field.dispatchEvent(new Event('change', { bubbles: true })); }, picker);
    await expect(node.$('[aria-label="Point input"]')).toBeDisabled();
    await expect(node.$('[aria-label="Delete curve point"]')).toBeDisabled();
    await pointer(await node.$('.curve-editor svg'), 'pointerdown', 100, 60, true);
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== baseline);
    await expect(node.$('textarea')).toHaveValue('0,0;1,1');
    await undo(baseline);
    await expect(node.$('textarea')).toHaveValue('0,0;0.5,0.5;1,1');
    await remove(node, 'Curve');
  });
  it('groups keyboard nudges and protects endpoints from deletion', async () => {
    const node = await add('core.curve', 'Curve');
    const beforeAdd = await invoke('workflow_hash');
    await browser.execute((element) => element.click(), await node.$('[aria-label="Add curve point"]'));
    await expect(node.$('textarea')).toHaveValue('0,0;0.5,0.5;1,1');
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== beforeAdd);
    const before = await invoke('workflow_hash');
    const svg = await node.$('.curve-editor svg');
    await browser.execute((element) => {
      for (const repeat of [false, true, true]) element.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp', repeat, bubbles: true, cancelable: true }));
    }, svg);
    expect(await invoke('workflow_hash')).toBe(before);
    await browser.execute((element) => element.dispatchEvent(new KeyboardEvent('keyup', { key: 'ArrowUp', bubbles: true })), svg);
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
    await undo(before);
    await expect(node.$('textarea')).toHaveValue('0,0;0.5,0.5;1,1');
    const endpoint = await node.$('.curve-editor__point');
    await browser.execute((element) => {
      element.focus();
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true }));
    }, endpoint);
    expect(await invoke('workflow_hash')).toBe(before);
    expect((await node.$$('.curve-editor__point')).length).toBe(3);
    await remove(node, 'Curve');
  });
});
