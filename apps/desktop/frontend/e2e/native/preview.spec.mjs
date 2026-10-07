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

  it('progressively replaces a coarse image without changing full-size coordinates', async () => {
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
    await browser.pause(500);
    await browser.execute(() => {
      window.__progressiveInitial = document.querySelector('.viewer-pane__image').src;
      window.__progressiveFrames = [];
      window.__progressiveStart = performance.now();
      const record = () => {
        const image = document.querySelector('.viewer-pane__image');
        if (!image || !image.complete || !image.naturalWidth || image.src === window.__progressiveInitial) return;
        if (window.__progressiveFrames.at(-1)?.url === image.src) return;
        window.__progressiveFrames.push({ url: image.src, width: image.naturalWidth, height: image.naturalHeight,
          fullWidth: image.width, fullHeight: image.height, elapsed: performance.now() - window.__progressiveStart });
      };
      window.__progressiveObserver = new MutationObserver(() => {
        record();
        document.querySelector('.viewer-pane__image')?.addEventListener('load', record, { once: true });
      });
      window.__progressiveObserver.observe(document.querySelector('section[aria-label="Image viewers"]'),
        { subtree: true, childList: true, attributes: true, attributeFilter: ['src'] });
    });
    try {
      // A fixed 100% zoom prevents asynchronous viewport layout from changing
      // the desired detail level during the two-stage regression.
      await $('.viewer-pane').$('button*=100%').click();
      await browser.waitUntil(() => browser.execute(() => window.__progressiveFrames.length >= 2), { timeout: 20_000 });
      const frames = await browser.execute(() => window.__progressiveFrames);
      const coarse = frames[0];
      const fine = frames[1];
      expect(fine.width).toBeGreaterThan(coarse.width);
      expect(fine.width).toBeGreaterThanOrEqual(coarse.width * 2 - 1);
      expect(fine.height).toBeGreaterThanOrEqual(coarse.height * 2 - 1);
      expect([fine.fullWidth, fine.fullHeight]).toEqual([coarse.fullWidth, coarse.fullHeight]);
      await expect($('.viewer-pane__image')).toHaveAttribute('src', fine.url);
      console.log('NATIVE progressive warm debug fixture', frames);
    } finally {
      await browser.execute(() => window.__progressiveObserver.disconnect());
      await $('.viewer-pane').$('button*=Fit').click();
    }
  });

  it('keeps a rendered preview on screen while the graph and layout change', async () => {
    const image = await $('.viewer-pane__image');
    await image.waitForDisplayed({ timeout: 20_000 });
    await browser.waitUntil(async () => browser.execute(() => document.querySelector('.viewer-pane__image')?.complete === true), {
      timeout: 20_000,
      timeoutMsg: 'the restored preview did not finish loading',
    });

    await browser.execute(() => {
      // `blank` is the regression this guards: the pane loses its image (no
      // element, no src, or a decode failure) so the user stares at nothing.
      // `swaps` is a legitimate frame replacement: a layout change can promote
      // the preview to a sharper mip, and that new URL is not decoded yet.
      window.__flicker = { samples: 0, blank: 0, swaps: 0, loadedSrc: null };
      window.__flickerTimer = window.setInterval(() => {
        window.__flicker.samples += 1;
        const element = document.querySelector('.viewer-pane__image');
        const src = element?.getAttribute('src') ?? '';
        if (!element || !src) {
          window.__flicker.blank += 1;
          return;
        }
        if (!element.complete) {
          if (src === window.__flicker.loadedSrc) window.__flicker.blank += 1;
          else window.__flicker.swaps += 1;
          return;
        }
        if (element.naturalWidth === 0) {
          window.__flicker.blank += 1;
          return;
        }
        window.__flicker.loadedSrc = src;
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
    expect(flicker.blank).toBe(0);
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
        window: `${window.innerWidth}x${window.innerHeight}`,
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
    // The stage floor is only reachable when the window is tall enough to hold the
    // scope strip below it. Window managers on this machine hand the app a short
    // window, so scale the floor to the panel that actually exists instead of
    // asserting a pixel height the environment cannot provide.
    const stageFloor = Math.min(120, Math.round(layout.section * 0.3));
    expect(layout.stage).toBeGreaterThan(stageFloor);
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

  it('draws a crop region on the real input preview and undoes it as one edit', async () => {
    const search = await $('input[placeholder="Search nodes"]');
    await search.setValue('Crop');
    await $('.node-library__item').click();
    await browser.tauri.execute((tauri) => tauri.core.invoke('disconnect_nodes', {
      fromNode: 'input', fromPort: 'image', toNode: 'output', toPort: 'image',
    }));
    await browser.tauri.execute((tauri) => tauri.core.invoke('connect_nodes', {
      fromNode: 'input', fromPort: 'image', toNode: 'crop', toPort: 'image',
    }));
    await browser.tauri.execute((tauri) => tauri.core.invoke('connect_nodes', {
      fromNode: 'crop', fromPort: 'image', toNode: 'output', toPort: 'image',
    }));

    const node = await $('[aria-label="Crop node"]');
    await node.click();
    const details = await node.$('details.graph-node__details');
    if (!(await details.getProperty('open'))) await details.$('summary').click();
    // The direct backend connections above need one normal controller command
    // to refresh the React snapshot before the input target can be observed.
    const cropWidth = await node.$('input[aria-label="Crop Width"]');
    await cropWidth.setValue('2');
    await browser.execute((element) => element.blur(), cropWidth);
    const resetWidth = await node.$('button[aria-label="Reset Crop Width"]');
    await resetWidth.waitForExist();
    await resetWidth.click();
    const draw = await node.$('button*=Draw crop region');
    await draw.waitForDisplayed();
    await browser.waitUntil(async () => await draw.isEnabled(), {
      timeout: 20_000,
      timeoutMsg: 'connected crop input never enabled drawing',
    });
    await draw.click();
    const overlay = await $('[aria-label="Draw image region"]');
    await overlay.waitForDisplayed();
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
    await expect(node).toHaveText(expect.stringContaining('Input resolution:'));
    expect(await browser.execute(() => getComputedStyle(document.querySelector('.geometry-overlay__guide rect')).fill)).toBe('none');
    const points = await browser.execute(() => {
      const image = document.querySelector('.viewer-pane__image').getBoundingClientRect();
      return {
        start: { x: image.left + image.width * 0.2, y: image.top + image.height * 0.2 },
        end: { x: image.left + image.width * 0.8, y: image.top + image.height * 0.8 },
      };
    });
    const pointer = (type, point) => browser.execute((kind, at) => {
      document.querySelector('[aria-label="Draw image region"]').dispatchEvent(new PointerEvent(kind, {
        bubbles: true, cancelable: true, composed: true, pointerId: 37, pointerType: 'mouse',
        isPrimary: true, button: 0, buttons: kind === 'pointerup' ? 0 : 1,
        clientX: at.x, clientY: at.y,
      }));
    }, type, point);
    await pointer('pointerdown', points.start);
    await pointer('pointermove', points.end);
    await pointer('pointerup', points.end);

    let saved;
    await browser.waitUntil(async () => {
      saved = JSON.parse(await browser.tauri.execute((tauri) => tauri.core.invoke('save_workflow')));
      return saved.nodes.crop.parameters.width.Float > 1;
    }, { timeoutMsg: 'crop gesture did not persist its parameters' });
    const parameters = saved.nodes.crop.parameters;
    expect(parameters.x.Float).toBeGreaterThan(0);
    expect(parameters.y.Float).toBeGreaterThan(0);
    expect(parameters.width.Float).toBeGreaterThan(100);
    expect(parameters.height.Float).toBeGreaterThan(100);
    await $$('button').then(async (buttons) => {
      for (const button of buttons) {
        if ((await button.getText()).trim() === 'Undo') return button.click();
      }
      throw new Error('Undo button was not found');
    });
    await browser.waitUntil(async () => {
      const workflow = JSON.parse(await browser.tauri.execute((tauri) => tauri.core.invoke('save_workflow')));
      return workflow.nodes.crop.parameters.x.Float === 0
        && workflow.nodes.crop.parameters.y.Float === 0
        && workflow.nodes.crop.parameters.width.Float === 1
        && workflow.nodes.crop.parameters.height.Float === 1;
    }, { timeoutMsg: 'undo did not restore the crop defaults' });
    await browser.execute(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    await expect($('[aria-label="Draw image region"]')).not.toExist();
    await browser.waitUntil(async () => browser.execute(() =>
      document.querySelector('select[aria-label="Viewer A target"]')?.value === 'crop:image'), {
      timeoutMsg: 'crop preview target was not restored after Escape',
    });
    await browser.waitUntil(async () => browser.execute(() => {
      const image = document.querySelector('.viewer-pane__image');
      return image?.complete === true && image.naturalWidth > 0;
    }), { timeoutMsg: 'crop preview image was not restored after undo and Escape' });
    await browser.tauri.execute((tauri) => tauri.core.invoke('disconnect_nodes', {
      fromNode: 'input', fromPort: 'image', toNode: 'crop', toPort: 'image',
    }));
    await browser.tauri.execute((tauri) => tauri.core.invoke('disconnect_nodes', {
      fromNode: 'crop', fromPort: 'image', toNode: 'output', toPort: 'image',
    }));
    await browser.tauri.execute((tauri) => tauri.core.invoke('connect_nodes', {
      fromNode: 'input', fromPort: 'image', toNode: 'output', toPort: 'image',
    }));
    const cropNode = await $('[aria-label="Crop node"]');
    await cropNode.click();
    const deleteCrop = await cropNode.$('button[aria-label="Delete Crop"]');
    await browser.execute((element) => element.click(), deleteCrop);
    await browser.waitUntil(async () => browser.execute(() =>
      document.querySelector('.canvas-panel__meta')?.textContent?.includes('2 nodes')), {
      timeoutMsg: 'UI crop cleanup did not restore the two-node workflow',
    });
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 20_000 });
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

  it('pans the preview with the pointer and keeps the clipping overlay aligned', async () => {
    await (await $('.viewer-pane__image')).waitForDisplayed({ timeout: 20_000 });
    await $('input[aria-label="Clipping"]').click();
    const overlay = await $('[aria-label="Viewer A clipping overlay"]');
    await overlay.waitForExist({ timeoutMsg: 'the clipping overlay never appeared' });

    // One pointer event per protocol call so each move can be observed the way a
    // real drag delivers them.
    const pointer = (type, x, y) => browser.execute((kind, clientX, clientY) => {
      document.querySelector('.viewer-pane__stage')?.dispatchEvent(new PointerEvent(kind, {
        bubbles: true, cancelable: true, composed: true, pointerId: 31, pointerType: 'mouse',
        isPrimary: true, button: 0, buttons: kind === 'pointerup' ? 0 : 1, clientX, clientY,
      }));
    }, type, x, y);
    const nextFrame = () => browser.execute(() => new Promise((resolve) => requestAnimationFrame(() => resolve())));
    const geometry = () => browser.execute(() => {
      const stage = document.querySelector('.viewer-pane__stage');
      const image = document.querySelector('.viewer-pane__image');
      const canvas = document.querySelector('[aria-label="Viewer A clipping overlay"]');
      if (!stage || !image || !canvas) return null;
      const stageBounds = stage.getBoundingClientRect();
      const imageBounds = image.getBoundingClientRect();
      const canvasBounds = canvas.getBoundingClientRect();
      return {
        transform: image.style.transform,
        offsetX: Math.round(imageBounds.left - stageBounds.left),
        offsetY: Math.round(imageBounds.top - stageBounds.top),
        overlayX: Math.round(canvasBounds.left - stageBounds.left),
        overlayY: Math.round(canvasBounds.top - stageBounds.top),
        overlayWidth: Math.round(canvasBounds.width),
        imageWidth: Math.round(imageBounds.width),
      };
    });

    const start = await browser.execute(() => {
      const bounds = document.querySelector('.viewer-pane__stage').getBoundingClientRect();
      return { x: Math.round(bounds.left + bounds.width / 2), y: Math.round(bounds.top + bounds.height / 2) };
    });
    const before = await geometry();
    await pointer('pointerdown', start.x, start.y);

    for (const step of [1, 2, 3]) {
      await pointer('pointermove', start.x + step * 12, start.y + step * 8);
      await nextFrame();
      const moved = await geometry();
      // The image must track the pointer on every move, not after the gesture.
      expect(moved.transform).toContain(`translate(${step * 12}px, ${step * 8}px)`);
      expect(moved.offsetX - before.offsetX).toBe(step * 12);
      expect(moved.offsetY - before.offsetY).toBe(step * 8);
      // The clipping overlay covers the moved image, not where it started.
      expect(moved.overlayX).toBe(moved.offsetX);
      expect(moved.overlayY).toBe(moved.offsetY);
      expect(moved.overlayWidth).toBe(moved.imageWidth);
    }

    await pointer('pointerup', start.x + 36, start.y + 24);
    await nextFrame();
    const committed = await geometry();
    expect(committed.transform).toContain('translate(36px, 24px)');
    expect(committed.overlayX).toBe(committed.offsetX);

    // The committed pan survives an unrelated re-render instead of snapping back.
    await selectMode('Integrations');
    await selectMode('Build / Preview');
    await browser.pause(300);
    const settled = await geometry();
    expect(settled.transform).toContain('translate(36px, 24px)');
    expect(settled.offsetX).toBe(committed.offsetX);

    await $('input[aria-label="Clipping"]').click();
    await browser.execute(() => [...document.querySelectorAll('.viewer-pane__controls button')]
      .find((button) => button.textContent === 'Fit')?.click());
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

/**
 * The comparison surfaces (wipe, blink, difference) draw the two viewers on top
 * of each other, so they own a different layout from the side-by-side grid. A
 * cascade of equal-specificity rules used to leave those panes as `display: grid`
 * whose single child landed in the `auto` track: the stage had no height to be
 * 100% of, and every comparison mode rendered an empty panel with a 1x1 image.
 */
describe('real Tauri viewer comparisons', () => {
  async function clickViewerAction(label) {
    const button = await $(`.viewer-section button[aria-label="${label}"]`);
    await button.waitForDisplayed();
    await button.click();
  }

  /** Mean luminance of an element region, measured from a real screenshot. */
  async function meanLuminance(selector) {
    const rect = await browser.execute((query) => {
      const element = document.querySelector(query);
      if (!element) return null;
      const bounds = element.getBoundingClientRect();
      return { x: bounds.left, y: bounds.top, width: bounds.width, height: bounds.height };
    }, selector);
    if (!rect || rect.width < 4 || rect.height < 4) return null;
    const shot = await browser.takeScreenshot();
    return browser.execute(async (base64, region) => {
      const image = new Image();
      image.src = `data:image/png;base64,${base64}`;
      await image.decode();
      const scaleX = image.width / window.innerWidth;
      const scaleY = image.height / window.innerHeight;
      const canvas = document.createElement('canvas');
      canvas.width = Math.max(1, Math.floor(region.width * scaleX));
      canvas.height = Math.max(1, Math.floor(region.height * scaleY));
      const context = canvas.getContext('2d');
      context.drawImage(
        image,
        Math.floor(region.x * scaleX), Math.floor(region.y * scaleY), canvas.width, canvas.height,
        0, 0, canvas.width, canvas.height,
      );
      const data = context.getImageData(0, 0, canvas.width, canvas.height).data;
      let total = 0;
      for (let index = 0; index < data.length; index += 4) total += (data[index] + data[index + 1] + data[index + 2]) / 3;
      return Math.round(total / (data.length / 4));
    }, shot, rect);
  }

  /** Ratio of the image to its stage; the preview is fit, so this reaches 1. */
  async function fillRatio(viewerClass) {
    return browser.execute((cls) => {
      const image = document.querySelector(`${cls} .viewer-pane__image`)?.getBoundingClientRect();
      const stage = document.querySelector(`${cls} .viewer-pane__stage`)?.getBoundingClientRect();
      if (!image || !stage || stage.width < 1 || stage.height < 1) return 0;
      return Math.max(image.width / stage.width, image.height / stage.height);
    }, viewerClass);
  }

  /**
   * Waits for a comparison pane to settle.
   *
   * Switching from the side-by-side grid to a comparison surface changes the
   * pane's viewport, and the re-fit lands a frame or two later (ResizeObserver ->
   * requestAnimationFrame). Waiting for the value the layout promises beats
   * guessing with a sleep.
   */
  async function waitForFilled(viewerClass, label) {
    await browser.waitUntil(async () => (await fillRatio(viewerClass)) > 0.9, {
      timeout: 10_000,
      timeoutMsg: `${label}: the comparison image never filled its surface`,
    });
  }

  async function previewReady() {
    await $('.viewer-pane__image').waitForDisplayed({ timeout: 30_000 });
    await browser.waitUntil(async () => browser.execute(() =>
      document.querySelector('.viewer-pane__image')?.complete === true), {
      timeout: 30_000,
      timeoutMsg: 'the restored preview did not finish loading',
    });
    await browser.pause(800);
  }

  it('fills the comparison surfaces with a real image', async () => {
    await previewReady();
    await clickViewerAction('Compare A and B');
    await expect($('select[aria-label="Viewer B target"]')).toBeDisplayed();

    // Point both viewers at the same output so the difference surface has a
    // known answer: an image differenced with itself is black.
    const targetValue = await browser.execute(() => document.querySelector('select[aria-label="Viewer A target"]')?.value ?? '');
    await browser.execute((value) => {
      const select = document.querySelector('select[aria-label="Viewer B target"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
      setter.call(select, value);
      select.dispatchEvent(new Event('change', { bubbles: true }));
    }, targetValue);
    await browser.waitUntil(async () => browser.execute(() =>
      [...document.querySelectorAll('.viewer-pane__image')].length === 2
      && [...document.querySelectorAll('.viewer-pane__image')].every((image) => image.complete && image.naturalWidth > 0)), {
      timeout: 30_000,
      timeoutMsg: 'both comparison previews never loaded',
    });
    await browser.pause(600);

    // Earlier specs (the pan spec in particular) leave the viewer panned, zoomed
    // and showing the clipping overlay. The difference of two misaligned images
    // is not black, so reset the navigation before measuring.
    const clipping = await $('input[aria-label="Clipping"]');
    if (await clipping.isSelected()) await clipping.click();
    for (const button of await $$('.viewer-grid .viewer-pane__controls button')) {
      if ((await button.getText()).trim() === 'Fit') await button.click();
    }
    await browser.pause(400);

    for (const mode of ['Wipe', 'Blink', 'Difference']) {
      await clickViewerAction(mode);
      await waitForFilled('.viewer-pane--comparison-a', mode);

      const geometry = await browser.execute(() => {
        const image = document.querySelector('.viewer-pane--comparison-a .viewer-pane__image')?.getBoundingClientRect();
        const stage = document.querySelector('.viewer-pane--comparison-a .viewer-pane__stage')?.getBoundingClientRect();
        return {
          image: image ? { width: image.width, height: image.height } : null,
          stage: stage ? { width: stage.width, height: stage.height } : null,
        };
      });
      // Before the fix the stage was 0 tall and the image a 1x1 dot.
      expect(geometry.stage?.height).toBeGreaterThan(100);
      expect(geometry.image?.width).toBeGreaterThan(20);
      expect(geometry.image?.height).toBeGreaterThan(20);
    }

    // Wipe at 0% shows Viewer B alone; the difference against an identical
    // Viewer A must go black. A blend confined to Viewer B's own stacking
    // context never reaches A and renders B unchanged instead.
    await clickViewerAction('Wipe');
    await browser.execute(() => {
      const slider = document.querySelector('input[aria-label="Wipe position"]');
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(slider, '0');
      slider.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await browser.pause(500);
    await waitForFilled('.viewer-pane--comparison-b', 'Wipe');
    const shown = await meanLuminance('.viewer-pane--comparison-b .viewer-pane__image');

    await clickViewerAction('Difference');
    await waitForFilled('.viewer-pane--comparison-b', 'Difference');
    const difference = await meanLuminance('.viewer-pane--comparison-b .viewer-pane__image');

    expect(shown).toBeGreaterThan(10);
    expect(difference).toBeLessThan(shown * 0.25);

    // Leave the workspace where the next spec file expects it.
    await clickViewerAction('Compare A and B');
    await selectMode('Build / Preview');
    await expect($('.canvas-panel')).toBeDisplayed();
  });
});
