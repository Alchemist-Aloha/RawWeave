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

async function clickTopbarAction(label) {
  for (const button of await $$('.topbar__actions button')) {
    if (await button.getText() === label) {
      await button.click();
      return;
    }
  }
  throw new Error(`topbar action ${label} was not found`);
}

async function clickViewerAction(label) {
  const button = await $(`.viewer-section button[aria-label="${label}"]`);
  if (!(await button.isExisting())) throw new Error(`viewer action ${label} was not found`);
  await button.click();
}

describe('frontend surfaces in browser mode', () => {
  beforeEach(async () => {
    await browser.url('http://127.0.0.1:5178/');
    await $('nav[aria-label="Workspace mode"]').waitForDisplayed();
  });

  it('shows the graph editor and viewer', async () => {
    await expect($('main.app-shell--build')).toBeDisplayed();
    await expect($('aside.panel--library')).toBeDisplayed();
    await expect($('aside.panel--inspector')).toBeDisplayed();
    await expect($('nav[aria-label="Workflow breadcrumbs"]')).toBeDisplayed();
    await expect($('section[aria-label="Image viewers"]')).toBeDisplayed();
    await clickViewerAction('Wipe');
    await expect($('[aria-label="Viewer comparison"]')).toBeDisplayed();
    await clickViewerAction('Blink');
    await expect($('[aria-label="blink comparison"]')).toBeDisplayed();
    await clickViewerAction('Difference');
    await expect($('[aria-label="difference comparison"]')).toBeDisplayed();
    await clickViewerAction('Compare A and B');
    await clickViewerAction('Compare A and B');
    await expect($('[aria-label="Viewer B"]')).toBeDisplayed();
  });

  it('only offers the side-by-side layout toggle where it applies', async () => {
    await clickViewerAction('Compare A and B');
    await expect($('[aria-label="Viewer layout: side by side"]')).toBeDisplayed();
    await expect($('.viewer-grid--side-by-side')).toBeDisplayed();
    await $('[aria-label="Viewer layout: stacked"]').click();
    await expect($('.viewer-grid--split')).toBeDisplayed();

    // Wipe/blink/difference render their own surface, so a layout toggle there
    // would be a control with no effect.
    await clickViewerAction('Wipe');
    await expect($('[aria-label="wipe comparison"]')).toBeDisplayed();
    await expect($('[aria-label="Viewer layout: side by side"]')).not.toExist();
    await expect($('[aria-label="Viewer layout: stacked"]')).not.toExist();
  });

  it('adds a graph node and supports undo and redo', async () => {
    await $('.node-library__item').waitForExist();
    await $('input[placeholder="Search nodes"]').setValue('Exposure');
    await $('.node-library__item').waitForExist();
    await $('.node-library__item').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));
    await $('[aria-label="Exposure node"]').click();
    await expect($('aside.panel--inspector')).toHaveText(expect.stringContaining('Exposure'));
    await clickTopbarAction('Undo');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('0 nodes'));
    await clickTopbarAction('Redo');
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('1 nodes'));
  });

  it('opens the browser and queue surface', async () => {
    await selectMode('Browse / Queue');
    await expect($('main.app-shell--browse')).toBeDisplayed();
    await expect($('section[aria-label="File browser"]')).toBeDisplayed();
    await expect($('aside[aria-label="Working Queue"]')).toBeDisplayed();
    await expect($('section[aria-label="Image Sets"]')).toBeDisplayed();
    await $('input[aria-label="Filter files"]').setValue('jpg');
    await expect($('input[aria-label="Filter files"]')).toHaveValue('jpg');
  });

  it('opens batch configuration', async () => {
    await selectMode('Batch');
    await expect($('main.app-shell--batch')).toBeDisplayed();
    await expect($('section[aria-label="Batch"]')).toBeDisplayed();
    await expect($('select[aria-label="Batch output format"]')).toBeDisplayed();
    await expect($('input[aria-label="Batch destination"]')).toBeDisplayed();
    await expect($('section[aria-label="Batch checkpoint policy"]')).toBeDisplayed();
    await expect($('section[aria-label="Batch dry run"]')).toBeDisplayed();
    await expect($('section[aria-label="Batch diagnostics"]')).toBeDisplayed();
    await $('select[aria-label="Batch output format"]').selectByAttribute('value', 'png');
    await expect($('select[aria-label="Batch output format"]')).toHaveValue('png');
    await $('select[aria-label="Batch resolution"]').selectByAttribute('value', 'exact');
    await expect($('input[aria-label="Batch width"]')).toBeDisplayed();
    await expect($('input[aria-label="Batch height"]')).toBeDisplayed();
    await $('select[aria-label="Dry run subset"]').selectByAttribute('value', 'first-n');
    await expect($('input[aria-label="Dry run first N"]')).toBeDisplayed();
  });

  it('opens external host and AI provider settings', async () => {
    await selectMode('Integrations');
    await expect($('main.app-shell--integrations')).toBeDisplayed();
    await expect($('section[aria-label="External hosts"]')).toBeDisplayed();
    await expect($('section[aria-label="AI providers"]')).toBeDisplayed();
    await expect($('input[aria-label="Host id"]')).toBeDisplayed();
    await expect($('input[aria-label="AI provider id"]')).toBeDisplayed();
    await expect($('input[aria-label="Host executable"]')).toBeDisplayed();
    await expect($('select[aria-label="AI provider type"]')).toBeDisplayed();
  });

  it('opens and closes the keyboard shortcut surface', async () => {
    await $('details.topbar__more summary').click();
    await clickTopbarAction('Shortcuts');
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).toBeDisplayed();

    // A modal surface must be dismissible from the keyboard.
    await browser.keys('Escape');
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).not.toExist();

    // The same dialog reopens from the menu and closes from its own button.
    await clickTopbarAction('Shortcuts');
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).toBeDisplayed();
    await $('button[aria-label="Close keyboard shortcuts"]').click();
    await expect($('[role="dialog"][aria-label="Keyboard shortcuts"]')).not.toExist();
  });
});
