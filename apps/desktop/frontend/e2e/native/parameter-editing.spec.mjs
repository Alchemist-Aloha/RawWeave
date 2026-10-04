import { $, browser, expect } from '@wdio/globals';

const invoke = (command) => browser.tauri.execute((tauri, name) => tauri.core.invoke(name), command);

async function typeDraft(element, text) {
  await browser.execute((field, next) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(field, next);
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }, element, text);
}

// Self-contained: do not depend on the restored source graph or other specs.
describe('editing accelerators in the native WebView', () => {
  it('organizes native descriptors by task and searches collapsed categories', async () => {
    const labels = await browser.execute(() => [...document.querySelectorAll('.node-library__group-summary > span:first-child')].map((el) => el.textContent));
    expect(labels).toContain('Tone & exposure');
    expect(labels).toContain('Mask sources');
    expect(labels).toContain('AI editing');
    expect(labels).not.toContain('Core');
    expect(labels).not.toContain('Pro Tools');
    await browser.execute(() => {
      const group = [...document.querySelectorAll('.node-library__group')].find((el) => el.querySelector('summary').textContent.includes('Tone & exposure'));
      group.open = false;
      group.dispatchEvent(new Event('toggle'));
    });
    const search = await $('input[placeholder="Search nodes"]');
    await typeDraft(search, 'tone & exposure');
    await expect($('.node-library__group')).toHaveAttribute('open');
    await expect($('.node-library__item')).toBeDisplayed();
    await browser.saveScreenshot('./logs/node-categories-native.png');
    await typeDraft(search, '');
  });

  it('scrolls only the node list while the title and search stay visible', async () => {
    for (const height of [900, 600]) {
      await browser.setWindowSize(1280, height);
      const before = await browser.execute(() => {
        const panel = document.querySelector('.panel--library');
        const list = panel.querySelector('.node-library');
        return {
          title: panel.querySelector('.panel__heading').getBoundingClientRect().top,
          search: panel.querySelector('.search-field').getBoundingClientRect().top,
          scrollable: list.scrollHeight > list.clientHeight,
        };
      });
      expect(before.scrollable).toBe(true);
      const after = await browser.execute(() => {
        const panel = document.querySelector('.panel--library');
        const list = panel.querySelector('.node-library');
        list.scrollTop = list.scrollHeight;
        return {
          title: panel.querySelector('.panel__heading').getBoundingClientRect().top,
          search: panel.querySelector('.search-field').getBoundingClientRect().top,
          listScroll: list.scrollTop,
          panelScroll: panel.scrollTop,
        };
      });
      expect(after.listScroll).toBeGreaterThan(0);
      expect(after.panelScroll).toBe(0);
      expect(after.title).toBe(before.title);
      expect(after.search).toBe(before.search);
      await expect($('input[placeholder="Search nodes"]')).toBeDisplayed();
      if (height === 600) await browser.saveScreenshot('./logs/node-library-scroll.png');
      await browser.execute(() => { document.querySelector('.node-library').scrollTop = 0; });
    }
    await browser.setWindowSize(1280, 900);
  });

  it('reveals the collapsed node library and focuses search with Ctrl+K', async () => {
    await $('[aria-label="Collapse Nodes panel"]').click();
    await browser.execute(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true }));
    });
    const search = await $('input[placeholder="Search nodes"]');
    await search.waitForDisplayed();
    await browser.waitUntil(() => browser.execute(() => document.activeElement?.getAttribute('placeholder') === 'Search nodes'));
  });

  it('keeps drafts local, cancels Escape, and commits once on Enter', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await typeDraft(search, 'Exposure');
    await $('.node-library__item').click();
    const node = await $('[aria-label="Exposure node"]');
    await node.waitForExist();
    await browser.execute((element) => { element.querySelector('details').open = true; }, node);
    const field = await node.$('input[aria-label="Exposure"]');
    const before = await invoke('workflow_hash');
    for (const text of ['1', '1.2', '1.25']) await typeDraft(field, text);
    expect(await invoke('workflow_hash')).toBe(before);
    await expect(field).toHaveValue('1.25');
    await browser.execute((element) => {
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    }, field);
    await expect(field).toHaveValue('0');
    expect(await invoke('workflow_hash')).toBe(before);
    await typeDraft(field, '1.25');
    // Explicit focus events are needed by WebKitGTK's embedded test driver.
    await browser.execute((element) => {
      element.focus();
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      element.dispatchEvent(new FocusEvent('focusout', { bubbles: true }));
    }, field);
    await browser.waitUntil(async () => (await invoke('workflow_hash')) !== before);
    const committed = await invoke('workflow_hash');
    const saved = JSON.parse(await invoke('save_workflow'));
    expect(Object.values(saved.nodes).some((value) => value.parameters.exposure?.Float === 1.25)).toBe(true);
    await browser.execute((element) => {
      element.dispatchEvent(new KeyboardEvent('keydown', { key: 'z', ctrlKey: true, bubbles: true, cancelable: true }));
    }, search);
    expect(await invoke('workflow_hash')).toBe(committed);
    await typeDraft(search, '');
  });
});
