import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { $, $$, browser, expect } from '@wdio/globals';

async function invoke(command, args) {
  return browser.tauri.execute((tauri, command, args) => tauri.core.invoke(command, args), command, args);
}

async function selectMode(label) {
  for (const button of await $$('nav[aria-label="Workspace mode"] button')) {
    if (await button.getText() === label) {
      await button.click();
      return;
    }
  }
  throw new Error(`workspace mode ${label} was not found`);
}

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

  it('keeps a rendered preview on screen while the graph and layout change', async () => {
    const image = await $('.viewer-pane__image');
    await image.waitForDisplayed({ timeout: 20_000 });
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.viewer-pane__image')?.complete === true), {
      timeout: 20_000,
      timeoutMsg: 'the restored preview did not finish loading',
    });

    await browser.execute(() => {
      window.__flicker = { samples: 0, missing: 0 };
      window.__flickerTimer = window.setInterval(() => {
        window.__flicker.samples += 1;
        const element = document.querySelector('.viewer-pane__image');
        if (!element || !element.complete || element.naturalWidth === 0) window.__flicker.missing += 1;
      }, 25);
    });

    // Adding a node changes the canvas height, and dragging one publishes
    // editor state; both used to restart the preview and blank the pane.
    const search = await $('input[placeholder="Search nodes"]');
    await search.setValue('Exposure');
    const item = await $('.node-library__item');
    await item.waitForDisplayed();
    await item.click();
    await browser.pause(800);
    await browser.execute(async () => {
      const node = document.querySelector('.react-flow__node');
      if (!node) return;
      const rect = node.getBoundingClientRect();
      const x = rect.left + rect.width / 2;
      const y = rect.top + 12;
      const fire = (target, type, at) => target.dispatchEvent(new MouseEvent(type, {
        bubbles: true, cancelable: true, composed: true, view: window,
        detail: 1, button: 0, buttons: type === 'mouseup' ? 0 : 1, clientX: at.x, clientY: at.y,
      }));
      fire(node, 'mousedown', { x, y });
      for (let step = 1; step <= 10; step += 1) {
        fire(window, 'mousemove', { x: x + step * 6, y: y + step * 4 });
        await new Promise((resolve) => setTimeout(resolve, 40));
      }
      fire(window, 'mouseup', { x: x + 60, y: y + 40 });
    });
    await browser.pause(2500);

    const flicker = await browser.execute(() => {
      window.clearInterval(window.__flickerTimer);
      return window.__flicker;
    });
    expect(flicker.samples).toBeGreaterThan(20);
    expect(flicker.missing).toBe(0);
    await expect($('.viewer-pane__image')).toBeDisplayed();

    // Restore the shared graph for the specs that run after this one.
    await $('[aria-label="Exposure node"]').click();
    await $('[aria-label="Delete Exposure"]').click();
    await expect($('.canvas-panel__meta')).toHaveText(expect.stringContaining('2 nodes'));
  });

  it('lays the preview out without dead space', async () => {
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
    await browser.pause(1500);

    const layout = await browser.execute(() => {
      const section = document.querySelector('.viewer-section');
      const rows = getComputedStyle(section).gridTemplateRows.split(' ').map(Number.parseFloat);
      return {
        section: Math.round(section.getBoundingClientRect().height),
        rows,
        toolbar: Math.round(document.querySelector('.viewer-section__toolbar')?.getBoundingClientRect().height ?? 0),
        stage: Math.round(document.querySelector('.viewer-pane__stage')?.getBoundingClientRect().height ?? 0),
        scopes: Math.round(document.querySelector('.viewer-scopes')?.getBoundingClientRect().height ?? 0),
        tabs: document.querySelectorAll('.scope-tab').length,
      };
    });

    // One compact toolbar row, a tabbed scope strip, and no unused track.
    expect(layout.toolbar).toBeLessThan(60);
    expect(layout.tabs).toBe(8);
    expect(layout.stage).toBeGreaterThan(120);
    expect(layout.rows.reduce((total, value) => total + value, 0)).toBeGreaterThan(layout.section - 14);
  });

  it('fits the preview image and scope canvas inside the panel', async () => {
    // The fit contract: the rendered image and every scope drawing sit inside
    // their panel. Regression: a large frame overflowed the stage and the scope
    // card overflowed its tab panel, both clipped by the panel's overflow.
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.viewer-pane__image')?.complete === true), {
      timeout: 20_000,
      timeoutMsg: 'the restored preview did not finish loading',
    });
    await browser.pause(1200);

    const fits = await browser.execute(() => {
      const within = (selector, parentSelector, tolerance = 2) => {
        const child = document.querySelector(selector)?.getBoundingClientRect();
        const parent = document.querySelector(parentSelector)?.getBoundingClientRect();
        if (!child || !parent) return false;
        return child.left >= parent.left - tolerance
          && child.top >= parent.top - tolerance
          && child.right <= parent.right + tolerance
          && child.bottom <= parent.bottom + tolerance;
      };
      return {
        image: within('.viewer-pane__image', '.viewer-pane__stage'),
        card: within('.scope-card', '.scope-tabs__panel'),
        canvas: within('.scope-canvas', '.scope-tabs__panel'),
      };
    });

    expect(fits.image).toBe(true);
    expect(fits.card).toBe(true);
    expect(fits.canvas).toBe(true);
  });

  it('keeps the workflow graph visible after switching workspaces', async () => {
    const workspace = await $('.workbench');
    const flow = await $('.react-flow');
    await flow.waitForDisplayed();
    await $('.react-flow__node').waitForDisplayed();
    await $('.react-flow__edge-path').waitForExist();
    const handleOffsets = await browser.execute(() => [...document.querySelectorAll('.graph-node__port')]
      .map((port) => {
        const handle = port.querySelector('.react-flow__handle');
        if (!handle) return null;
        const row = port.getBoundingClientRect();
        const point = handle.getBoundingClientRect();
        return Math.abs((point.top + point.height / 2) - (row.top + row.height / 2));
      })
      .filter((offset) => offset !== null));
    expect(handleOffsets.length).toBeGreaterThan(0);
    for (const offset of handleOffsets) expect(offset).toBeLessThan(3);

    for (const mode of ['Browse / Queue', 'Batch', 'Integrations']) {
      await selectMode(mode);
      await selectMode('Build / Preview');
      await expect(workspace).toBeDisplayed();
      await expect(flow).toBeDisplayed();
      await expect($('.react-flow__node')).toBeDisplayed();
      const geometry = await browser.execute(() => {
        const canvas = document.querySelector('.flow-canvas')?.getBoundingClientRect();
        const node = document.querySelector('.react-flow__node')?.getBoundingClientRect();
        return { canvasWidth: canvas?.width ?? 0, canvasHeight: canvas?.height ?? 0, nodeWidth: node?.width ?? 0 };
      });
      expect(geometry.canvasWidth).toBeGreaterThan(100);
      expect(geometry.canvasHeight).toBeGreaterThan(100);
      expect(geometry.nodeWidth).toBeGreaterThan(100);
    }
  });
});


const batchDestination = mkdtempSync(join(tmpdir(), 'rawweave-batch-'));

async function batchState() {
  return browser.execute(() => document.querySelector('.batch-state')?.textContent ?? null);
}

/**
 * Selects an option on a React-controlled `<select>`.
 *
 * The embedded WebKitGTK driver does not deliver the pointer action that
 * `selectByAttribute` uses to activate an option, so the change is dispatched
 * through the native value setter instead.
 */
async function selectOption(selector, value) {
  const select = await $(selector);
  await browser.execute((element, next) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
    setter.call(element, next);
    element.dispatchEvent(new Event('change', { bubbles: true }));
  }, select, value);
  await expect(select).toHaveValue(value);
}

describe('real Tauri batch processing', () => {
  after(() => {
    rmSync(batchDestination, { recursive: true, force: true });
  });

  it('creates, preflights, dry runs, and completes a job for the restored workflow', async () => {
    await $('nav[aria-label="Workspace mode"]').waitForDisplayed();
    await selectMode('Batch');
    await expect($('section[aria-label="Batch"]')).toBeDisplayed();

    // The restored fixture is queued, so a job can be built without a folder dialog.
    await expect($('input[aria-label="Batch destination"]')).toBeDisplayed();
    await $('input[aria-label="Batch destination"]').setValue(batchDestination);
    await selectOption('select[aria-label="Batch output format"]', 'png');
    await expect($('input[aria-label="Batch destination"]')).toHaveValue(batchDestination);

    const create = await $('[aria-label="Create batch"]');
    await create.waitForEnabled();
    await create.click();

    // Pinning must accept the definition the frontend builds from the active
    // workflow; a hash or identity mismatch surfaces here as an error banner.
    await browser.waitUntil(async () => (await batchState()) === 'draft', {
      timeout: 15_000,
      timeoutMsg: 'the batch job was not created',
    });
    await expect($('.batch-panel__error')).not.toExist();

    await $('[aria-label="Run batch preflight"]').click();
    await browser.waitUntil(async () => (await batchState()) === 'draft', { timeout: 15_000 });
    await expect($('.batch-panel__error')).not.toExist();

    await $('[aria-label="Run dry run"]').click();
    await browser.waitUntil(async () => (await $('.batch-panel__result')).isExisting(), {
      timeout: 15_000,
      timeoutMsg: 'the dry run returned no plan',
    });
    await expect($('.batch-panel__result')).toHaveText(expect.stringContaining('1 item'));

    await $('[aria-label="Start batch"]').click();
    await browser.waitUntil(async () => {
      const state = await batchState();
      return state === 'completed' || state === 'failed' || state === 'cancelled';
    }, { timeout: 60_000, timeoutMsg: 'the batch job did not settle' });
    await expect($('.batch-state')).toHaveText('completed');
    await expect($('section[aria-label="Batch progress"]')).toHaveText(expect.stringContaining('1 / 1 items complete'));

    const written = readdirSync(batchDestination);
    expect(written).toHaveLength(1);
    expect(written[0]).toMatch(/\.png$/);
    const page = await invoke('list_directory', { path: batchDestination, offset: 0, limit: 25 });
    expect(page.entries.map((entry) => entry.name)).toEqual(written.sort());

    // The native workers share one application instance, so leave the workspace
    // where the next spec file expects it.
    await selectMode('Build / Preview');
    await expect($('.canvas-panel')).toBeDisplayed();
  });
});
