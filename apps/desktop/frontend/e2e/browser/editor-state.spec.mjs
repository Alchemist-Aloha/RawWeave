import { $, $$, browser, expect } from '@wdio/globals';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const fixture = (name) => resolve(dirname(fileURLToPath(import.meta.url)), '..', 'fixtures', name);
const imageFixture = resolve(dirname(fileURLToPath(import.meta.url)), '../../../../../test-data/images/common/pngsuite-rgb8.png');

async function fileInputs() {
  return $$('.topbar__actions input[type="file"]');
}

async function openImage() {
  const inputs = await fileInputs();
  await inputs[1].addValue(await browser.uploadFile(imageFixture));
}

async function clickTopbarAction(label) {
  for (const button of await $$('.topbar__actions button')) {
    if ((await button.getText()).trim() === label) {
      await button.click();
      return;
    }
  }
  throw new Error(`topbar action ${label} was not found`);
}

describe('editor state and persistence', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await browser.execute(() => window.localStorage.clear());
    await browser.refresh();
    await $('nav[aria-label="Workspace mode"]').waitForDisplayed();
  });

  afterEach(async () => {
    await browser.execute(() => window.localStorage.clear());
  });

  it('reports an unreadable workflow document and keeps the previous graph', async () => {
    await $('.node-library__item').waitForExist();
    await $('.node-library__item').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    const inputs = await fileInputs();
    await inputs[2].addValue(await browser.uploadFile(fixture('workflow-invalid.json')));

    const toast = await $('.error-toast[role="alert"]');
    await toast.waitForDisplayed();
    await expect(toast).toHaveText(expect.stringContaining('Editor operation failed'));
    // The failed load must not replace or empty the graph that was on screen.
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    await $('button[aria-label="Dismiss editor error"]').click();
    await expect($('.error-toast[role="alert"]')).not.toExist();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));
  });

  it('retargets the viewer when the previewed node is deleted', async () => {
    await openImage();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));

    const target = await $('select[aria-label="Viewer A target"]');
    await expect(target).toBeDisplayed();
    await browser.waitUntil(async () => (await target.getValue()) !== '', {
      timeoutMsg: 'opening an image did not select a preview output',
    });
    const selected = await browser.execute((element) => element.selectedOptions[0]?.textContent ?? '', target);
    expect(selected).toContain('Output');

    await $('[aria-label="Output node"]').click();
    await $('[aria-label="Delete Output"]').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    // The pane must not keep pointing at a node that no longer exists.
    await browser.waitUntil(async () => (await target.getValue()) !== '' && !(await browser.execute((element) => element.selectedOptions[0]?.textContent ?? '', target)).includes('Output'), {
      timeoutMsg: 'the viewer kept the deleted Output node as its target',
    });
  });

  it('remembers the lamp and the panel layout across a reload', async () => {
    await $('aside.panel--library').waitForDisplayed();
    const lamp = await $('button.lamp');
    await expect(lamp).toHaveAttribute('aria-pressed', 'false');
    await lamp.click();
    await $('[aria-label="Collapse Nodes panel"]').click();
    await expect($('[aria-label="Expand Nodes panel"]')).toBeDisplayed();

    await browser.refresh();
    await $('nav[aria-label="Workspace mode"]').waitForDisplayed();

    await expect($('button.lamp')).toHaveAttribute('aria-pressed', 'true');
    await expect($('[aria-label="Expand Nodes panel"]')).toBeDisplayed();
    await expect($('aside.panel--library')).not.toExist();

    // Restoring the panel must leave a usable canvas, not a collapsed shell.
    const canvas = await browser.execute(() => Math.round(document.querySelector('.flow-canvas')?.getBoundingClientRect().width ?? 0));
    expect(canvas).toBeGreaterThan(200);
  });

  it('supports the documented editor keyboard shortcuts', async () => {
    await $('.node-library__item').waitForExist();
    await $('.node-library__item').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    await browser.keys(['Control', 'k']);
    const focused = await browser.execute(() => document.activeElement?.getAttribute('placeholder') ?? '');
    expect(focused).toBe('Search nodes');

    // Focus the canvas first: a "?" typed into a text field is text, not a shortcut.
    await $('.react-flow__pane').click();
    await browser.keys('?');
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).toBeDisplayed();

    // An open modal owns the keyboard: Delete must not remove the node behind it.
    await browser.keys('Delete');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    await browser.keys('Escape');
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).not.toExist();

    await $('.react-flow__pane').click();
    await browser.keys('Delete');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 nodes'));
  });

  it('adds the first search result from the keyboard and undoes back to empty', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await search.click();
    await search.addValue('Expo');
    await browser.keys('Enter');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));

    await search.clearValue();
    for (let index = 0; index < 4; index += 1) await $(`.node-library__item`).click();

    const undo = await (async () => {
      for (const button of await $$('.topbar__actions button')) {
        if ((await button.getText()).trim() === 'Undo') return button;
      }
      throw new Error('Undo button not found');
    })();
    for (let index = 0; index < 5; index += 1) await undo.click();

    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 nodes'));
    await expect($('.canvas-empty')).toBeDisplayed();
    await expect(undo).toBeDisabled();
  });

  it('disables the batch actions while the queue is empty', async () => {
    for (const button of await $$('nav[aria-label="Workspace mode"] button')) {
      if ((await button.getText()) === 'Batch') { await button.click(); break; }
    }
    await $('section[aria-label="Batch"]').waitForDisplayed();
    await expect($('[aria-label="Create batch"]')).toBeDisabled();
    await expect($('[aria-label="Run dry run"]')).toBeDisabled();
    await expect($('[aria-label="Run batch preflight"]')).toBeDisabled();
  });

  it('saves the workflow without an error', async () => {
    await $('.node-library__item').waitForExist();
    await $('.node-library__item').click();
    await clickTopbarAction('Save workflow');
    await expect($('.canvas-panel__footer')).toHaveText(expect.stringContaining('Workflow saved'));
    await expect($('.error-toast[role="alert"]')).not.toExist();
  });
});
