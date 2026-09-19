import { useEffect, useState } from 'react';
import { BatchController } from '../batch/controller';
import {
  recipeBitDepthLabel,
  recipeCollisionLabel,
  recipeCompressionLabel,
  recipeFormatLabel,
  recipeSharpeningLabel,
  type BatchWorkflowContext,
} from '../batch/model';
import type {
  BatchColorSpace,
  BatchFormat,
  BatchItem,
  BatchMetadataPolicy,
  BatchRecipe,
  BatchResolution,
  BatchSharpening,
  BatchSubset,
} from '../batch/types';
import type { QueueItem } from '../browser/types';

export interface BatchPanelProps {
  controller: BatchController;
  queueItems: QueueItem[];
  workflow?: BatchWorkflowContext | null;
  onOpenItem?: (item: BatchItem) => void | Promise<void>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function isExactResolution(value: BatchResolution): value is { exact: { width: number; height: number } } {
  return typeof value === 'object' && 'exact' in value;
}

function isLongEdgeResolution(value: BatchResolution): value is { long_edge: number } {
  return typeof value === 'object' && 'long_edge' in value;
}

function defaultSubset(queueItems: QueueItem[]): BatchSubset {
  return queueItems.length > 0 ? { kind: 'all' } : { kind: 'all' };
}

function subsetLabel(subset: BatchSubset): string {
  switch (subset.kind) {
    case 'current-preview': return 'Current preview item';
    case 'test-set': return 'Test Set';
    case 'first-n': return 'First N items';
    case 'selected': return 'Selected items';
    case 'all': return 'All queued items';
  }
}

function formatResolution(value: BatchResolution): string {
  if (value === 'original') return 'original';
  if (isExactResolution(value)) return 'exact';
  return 'long_edge';
}

export function BatchPanel({ controller, queueItems, workflow, onOpenItem }: BatchPanelProps) {
  const [, setRevision] = useState(0);
  const [subset, setSubset] = useState<BatchSubset>(() => defaultSubset(queueItems));
  const [selectedIds, setSelectedIds] = useState<string[]>(() => queueItems.map((item) => item.id));
  const [currentPreviewId, setCurrentPreviewId] = useState(queueItems[0]?.id ?? '');
  const [firstN, setFirstN] = useState(1);

  useEffect(() => controller.subscribe(() => setRevision((revision) => revision + 1)), [controller]);
  useEffect(() => {
    setSelectedIds((ids) => ids.filter((id) => queueItems.some((item) => item.id === id)));
    if (!queueItems.some((item) => item.id === currentPreviewId)) setCurrentPreviewId(queueItems[0]?.id ?? '');
  }, [currentPreviewId, queueItems]);

  const recipe = controller.recipe;
  const job = controller.state.job;
  const items = job?.items ?? [];
  const selectedItemIds = subset.kind === 'selected' ? subset.itemIds : selectedIds;
  const failedItems = items.filter((item) => item.state === 'failed');
  const terminalCount = items.filter((item) => ['completed', 'skipped', 'failed', 'cancelled'].includes(item.state)).length;
  const progress = items.length === 0 ? 0 : Math.round((terminalCount / items.length) * 100);

  const updateRecipe = <K extends keyof BatchRecipe>(key: K, value: BatchRecipe[K]) => {
    controller.updateRecipe({ [key]: value } as Partial<BatchRecipe>);
  };

  const run = (action: () => Promise<unknown>, after?: (value: unknown) => void) => {
    void action().then(after).catch(() => undefined);
  };

  const changeSubset = (kind: BatchSubset['kind']) => {
    switch (kind) {
      case 'current-preview': setSubset({ kind, itemId: currentPreviewId }); break;
      case 'test-set': setSubset({ kind }); break;
      case 'first-n': setSubset({ kind, count: firstN }); break;
      case 'selected': setSubset({ kind, itemIds: [...selectedIds] }); break;
      case 'all': setSubset({ kind }); break;
    }
  };

  const handleResolution = (value: string) => {
    if (value === 'original') updateRecipe('resolution', 'original');
    else if (value === 'exact') updateRecipe('resolution', { exact: { width: 1, height: 1 } });
    else updateRecipe('resolution', { long_edge: 2048 });
  };

  const handleSharpening = (value: string) => {
    const sharpening: BatchSharpening = value === 'None'
      ? 'None'
      : { UnsharpMask: { radius: 1, amount: 0.5, threshold: 0 } };
    updateRecipe('sharpening', sharpening);
  };

  const resolution = recipe.resolution;
  const sharpening = recipe.sharpening;
  const sharpeningValues = sharpening === 'None' ? null : sharpening.UnsharpMask;
  const colorSpace = typeof recipe.colorSpace === 'string' ? recipe.colorSpace : 'named';
  const colorName = typeof recipe.colorSpace === 'string' ? '' : recipe.colorSpace.named;

  return (
    <section aria-label="Batch" className="batch-panel">
      <header className="batch-panel__heading">
        <div>
          <span className="eyebrow">Batch processing</span>
          <strong>{job ? `Batch ${job.id}` : 'Batch'}</strong>
        </div>
        <span className={`batch-state batch-state--${job?.state ?? 'draft'}`}>{job?.state ?? 'draft'}</span>
      </header>

      {!job && (
        <div className="batch-panel__create">
          <span>{queueItems.length} queued item{queueItems.length === 1 ? '' : 's'}</span>
          <button
            aria-label="Create batch"
            className="button button--primary"
            disabled={!workflow || queueItems.length === 0 || controller.state.loading}
            onClick={() => workflow && run(() => controller.createFromQueue(queueItems, workflow, subset, recipe))}
            type="button"
          >
            Create batch
          </button>
        </div>
      )}

      <div className="batch-panel__recipe">
        <h3>Output recipe</h3>
        <div className="batch-panel__fields">
          <label>Format
            <select aria-label="Batch output format" value={recipe.format} onChange={(event) => updateRecipe('format', event.target.value as BatchFormat)}>
              <option value="jpeg">{recipeFormatLabel('jpeg')}</option>
              <option value="png">{recipeFormatLabel('png')}</option>
              <option value="tiff">{recipeFormatLabel('tiff')}</option>
              <option value="open_exr">{recipeFormatLabel('open_exr')}</option>
            </select>
          </label>
          <label>Resolution
            <select aria-label="Batch resolution" value={formatResolution(resolution)} onChange={(event) => handleResolution(event.target.value)}>
              <option value="original">Original</option>
              <option value="exact">Exact dimensions</option>
              <option value="long_edge">Long edge</option>
            </select>
          </label>
          {isExactResolution(resolution) && <>
            <label>Width<input aria-label="Batch width" min="1" type="number" value={resolution.exact.width} onChange={(event) => updateRecipe('resolution', { exact: { ...resolution.exact, width: Math.max(1, Number(event.target.value) || 1) } })} /></label>
            <label>Height<input aria-label="Batch height" min="1" type="number" value={resolution.exact.height} onChange={(event) => updateRecipe('resolution', { exact: { ...resolution.exact, height: Math.max(1, Number(event.target.value) || 1) } })} /></label>
          </>}
          {isLongEdgeResolution(resolution) && <label>Long edge<input aria-label="Batch long edge" min="1" type="number" value={resolution.long_edge} onChange={(event) => updateRecipe('resolution', { long_edge: Math.max(1, Number(event.target.value) || 1) })} /></label>}
          <label>Bit depth
            <select aria-label="Batch bit depth" value={recipe.bitDepth} onChange={(event) => updateRecipe('bitDepth', event.target.value as BatchRecipe['bitDepth'])}>
              <option value="eight">{recipeBitDepthLabel('eight')}</option>
              <option value="sixteen">{recipeBitDepthLabel('sixteen')}</option>
              <option value="float32">{recipeBitDepthLabel('float32')}</option>
            </select>
          </label>
          <label>Color space
            <select aria-label="Batch color space" value={colorSpace} onChange={(event) => updateRecipe('colorSpace', event.target.value === 'named' ? { named: colorName || 'Custom' } : event.target.value as BatchColorSpace)}>
              <option value="srgb">sRGB</option>
              <option value="linear_srgb">Linear sRGB</option>
              <option value="display_p3">Display P3</option>
              <option value="named">Named</option>
            </select>
          </label>
          {colorSpace === 'named' && <label>Color name<input aria-label="Batch named color space" value={colorName} onChange={(event) => updateRecipe('colorSpace', { named: event.target.value })} /></label>}
          <label>ICC profile<input aria-label="Batch ICC profile" value={recipe.iccProfile ?? ''} onChange={(event) => updateRecipe('iccProfile', event.target.value || null)} /></label>
          <label>OCIO transform<input aria-label="Batch OCIO transform" value={recipe.ocioTransform ?? ''} onChange={(event) => updateRecipe('ocioTransform', event.target.value || null)} /></label>
          <label>Metadata
            <select aria-label="Batch metadata policy" value={recipe.metadataPolicy} onChange={(event) => updateRecipe('metadataPolicy', event.target.value as BatchMetadataPolicy)}>
              <option value="preserve">Preserve</option><option value="strip">Strip</option><option value="sidecar">Sidecar</option>
            </select>
          </label>
          <label>Sharpening
            <select aria-label="Batch sharpening" value={recipeSharpeningLabel(sharpening)} onChange={(event) => handleSharpening(event.target.value === 'None' ? 'None' : 'UnsharpMask')}>
              <option value="None">None</option><option value="Unsharp mask">Unsharp mask</option>
            </select>
          </label>
          {sharpeningValues && <>
            <label>Radius<input aria-label="Batch sharpening radius" min="0" type="number" value={sharpeningValues.radius} onChange={(event) => updateRecipe('sharpening', { UnsharpMask: { ...sharpeningValues, radius: Math.max(0, Number(event.target.value) || 0) } })} /></label>
            <label>Amount<input aria-label="Batch sharpening amount" min="0" step="0.1" type="number" value={sharpeningValues.amount} onChange={(event) => updateRecipe('sharpening', { UnsharpMask: { ...sharpeningValues, amount: Math.max(0, Number(event.target.value) || 0) } })} /></label>
            <label>Threshold<input aria-label="Batch sharpening threshold" min="0" step="0.1" type="number" value={sharpeningValues.threshold} onChange={(event) => updateRecipe('sharpening', { UnsharpMask: { ...sharpeningValues, threshold: Math.max(0, Number(event.target.value) || 0) } })} /></label>
          </>}
          <label>Quality<input aria-label="Batch quality" max="100" min="1" type="number" value={recipe.quality} onChange={(event) => updateRecipe('quality', Math.min(100, Math.max(1, Number(event.target.value) || 1)))} /></label>
          <label>Compression
            <select aria-label="Batch compression" value={recipe.compression} onChange={(event) => updateRecipe('compression', event.target.value as BatchRecipe['compression'])}>
              <option value="default">{recipeCompressionLabel('default')}</option><option value="fast">{recipeCompressionLabel('fast')}</option><option value="best">{recipeCompressionLabel('best')}</option><option value="lossless">{recipeCompressionLabel('lossless')}</option>
            </select>
          </label>
          <label>Destination<input aria-label="Batch destination" value={recipe.destination} onChange={(event) => updateRecipe('destination', event.target.value)} /></label>
          <label>Filename template<input aria-label="Batch filename template" value={recipe.filenameTemplate} onChange={(event) => updateRecipe('filenameTemplate', event.target.value)} /></label>
          <label>Collision policy
            <select aria-label="Batch collision policy" value={recipe.collisionPolicy} onChange={(event) => updateRecipe('collisionPolicy', event.target.value as BatchRecipe['collisionPolicy'])}>
              <option value="error">{recipeCollisionLabel('error')}</option><option value="skip">{recipeCollisionLabel('skip')}</option><option value="overwrite">{recipeCollisionLabel('overwrite')}</option><option value="suffix">{recipeCollisionLabel('suffix')}</option>
            </select>
          </label>
        </div>
      </div>

      <section className="batch-panel__dry-run" aria-label="Batch dry run">
        <h3>Dry run</h3>
        <div className="batch-panel__fields">
          <label>Subset
            <select aria-label="Dry run subset" value={subset.kind} onChange={(event) => changeSubset(event.target.value as BatchSubset['kind'])}>
              <option value="current-preview">Current preview item</option><option value="test-set">Test Set</option><option value="first-n">First N items</option><option value="selected">Selected items</option><option value="all">All queued items</option>
            </select>
          </label>
          {subset.kind === 'current-preview' && <label>Preview item
            <select aria-label="Dry run current preview" value={currentPreviewId} onChange={(event) => { setCurrentPreviewId(event.target.value); setSubset({ kind: 'current-preview', itemId: event.target.value }); }}>
              {queueItems.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>}
          {subset.kind === 'first-n' && <label>Count<input aria-label="Dry run first N" min="0" type="number" value={firstN} onChange={(event) => { const count = Math.max(0, Math.trunc(Number(event.target.value) || 0)); setFirstN(count); setSubset({ kind: 'first-n', count }); }} /></label>}
        </div>
        <div className="batch-panel__selection">
          {queueItems.map((item) => <label key={item.id}><input aria-label={`Dry run select ${item.name}`} checked={selectedIds.includes(item.id)} onChange={(event) => { const next = event.target.checked ? [...selectedIds, item.id] : selectedIds.filter((id) => id !== item.id); setSelectedIds(next); if (subset.kind === 'selected') setSubset({ kind: 'selected', itemIds: next }); }} type="checkbox" />{item.name}</label>)}
        </div>
        <button aria-label="Run dry run" className="button button--quiet" disabled={!job || controller.state.loading} onClick={() => run(() => controller.dryRun(subset))} type="button">Run dry run: {subsetLabel(subset)}</button>
        {controller.state.dryRun && <p className="batch-panel__result">Revision {controller.state.dryRun.workflowRevision} · {controller.state.dryRun.itemIds.length} item{controller.state.dryRun.itemIds.length === 1 ? '' : 's'}</p>}
      </section>

      <section aria-label="Batch diagnostics" className="batch-panel__diagnostics">
        <h3>Preflight diagnostics</h3>
        <button aria-label="Run batch preflight" className="button button--quiet" disabled={!job || controller.state.loading} onClick={() => run(() => controller.preflight())} type="button">Run preflight</button>
        {controller.state.diagnostics.length === 0 && <p className="empty-state empty-state--compact">No diagnostics yet.</p>}
        <ul>{controller.state.diagnostics.map((diagnostic, index) => <li className={`diagnostic diagnostic--${diagnostic.severity}`} key={`${diagnostic.code}-${index}`}><strong>{diagnostic.severity}</strong><span>{diagnostic.message}</span></li>)}</ul>
      </section>

      {job && <>
        <section className="batch-panel__operations" aria-label="Batch operations">
          <button aria-label="Start batch" className="button button--primary" disabled={controller.state.loading || job.state === 'running' || job.state === 'completed'} onClick={() => run(() => controller.start())} type="button">Start</button>
          <button aria-label="Pause batch" className="button button--quiet" disabled={controller.state.loading || job.state !== 'running'} onClick={() => run(() => controller.pause())} type="button">Pause</button>
          <button aria-label="Resume batch" className="button button--quiet" disabled={controller.state.loading || job.state !== 'paused'} onClick={() => run(() => controller.resume())} type="button">Resume</button>
          <button aria-label="Cancel batch" className="button button--quiet" disabled={controller.state.loading || ['completed', 'cancelled'].includes(job.state)} onClick={() => run(() => controller.cancel())} type="button">Cancel</button>
          <button aria-label="Retry failed" className="button button--quiet" disabled={controller.state.loading || !failedItems.length} onClick={() => run(() => controller.retryFailed())} type="button">Retry failed</button>
          <button aria-label="Retry selected" className="button button--quiet" disabled={controller.state.loading || !selectedItemIds.length} onClick={() => run(() => controller.retrySelected(selectedItemIds))} type="button">Retry selected</button>
          <button aria-label="Skip selected" className="button button--quiet" disabled={controller.state.loading || !selectedItemIds.length} onClick={() => run(() => controller.skip(selectedItemIds))} type="button">Skip selected</button>
          <button aria-label="Open failed item" className="button button--quiet" disabled={controller.state.loading || !failedItems.length} onClick={() => failedItems[0] && run(() => controller.openFailedItem(failedItems[0].id), (opened) => opened && onOpenItem?.(opened as BatchItem))} type="button">Open failed</button>
        </section>
        <section aria-label="Batch progress" className="batch-panel__progress">
          <div><strong>{progress}%</strong><span>{terminalCount} / {items.length} items complete</span></div>
          <progress max="100" value={progress}>{progress}%</progress>
          <ul>{items.map((item) => <li key={item.id}><span>{item.displayName}</span><span>{item.state}</span>{item.failure && <small>{item.failure}</small>}{item.state === 'failed' && <button aria-label="Open failed item" className="button button--quiet" onClick={() => run(() => controller.openFailedItem(item.id), (opened) => opened && onOpenItem?.(opened as BatchItem))} type="button">Open</button>}</li>)}</ul>
        </section>
      </>}

      {controller.state.error && <p className="batch-panel__error" role="alert">{errorMessage(controller.state.error)}</p>}
    </section>
  );
}
