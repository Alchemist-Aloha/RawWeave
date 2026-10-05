import { readFileSync } from 'node:fs';
import { $, browser, expect } from '@wdio/globals';

describe('physical curve quantities', () => {
  it('labels image curves as channel levels, not luminance or spatial positions', async () => {
    await browser.execute(() => {
      const field = document.querySelector('input[placeholder="Search nodes"]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, 'core.curves');
      field.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await browser.execute(() => [...document.querySelectorAll('.node-library__item')].find((item) => item.querySelector('small')?.textContent === 'core.curves').click());
    const node = await $('[aria-label="Curves node"]');
    await node.waitForExist();
    await browser.execute((element) => { element.querySelector('details').open = true; }, node);
    await expect(node.$('.curve-preview__axis--x')).toHaveText('RGB level before curve');
    await expect(node.$('.curve-preview__axis--y')).toHaveText('RGB level after curve');
    expect(await node.$('.curve-preview').getText()).toContain('dark to bright');
    expect(await node.$('.curve-preview').getText()).toContain('not luminance');
  });
  it('uses ISO and fractional recovery strength for the actual RAW logic workflow', async () => {
    const workflow = readFileSync(new URL('../../../../../examples/workflows/iso-adaptive-raw.json', import.meta.url), 'utf8');
    await browser.execute((json) => {
      // The first JSON input imports blueprints; the final header input opens workflows.
      const input = document.querySelector('header input[type="file"][accept="application/json,.json"]:last-child');
      const transfer = new DataTransfer();
      transfer.items.add(new File([json], 'iso-adaptive-raw.json', { type: 'application/json' }));
      input.files = transfer.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
    }, workflow);
    await browser.waitUntil(async () => {
      const serialized = await browser.tauri.execute((tauri) => tauri.core.invoke('save_workflow'));
      return Boolean(JSON.parse(serialized).nodes['15-recovery']);
    });
    const node = await $('[aria-label="Curve node"]');
    await node.waitForExist();
    await browser.execute((element) => { element.querySelector('details').open = true; }, node);
    await expect(node.$('.curve-preview__axis--x')).toHaveText('ISO');
    await expect(node.$('.curve-preview__axis--y')).toHaveText('Recovery Strength (fraction)');
    expect(await node.$('.curve-preview').getText()).toContain('not image brightness');
    await $('.react-flow__controls-fitview').click();
    await browser.saveScreenshot('./logs/iso-physical-axes-native.png');
  });
});
