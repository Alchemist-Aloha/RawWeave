import { useCallback, useEffect, useMemo, useState } from 'react';
import { BrowserController, type BrowserPlatform, type BrowserState } from './controller';
import { breadcrumbSegments, filterBrowserEntries, sortBrowserEntries } from './model';
import { QueueController, type QueueState } from '../queue/controller';
import { createBrowserPlatform } from '../platform/browser';
import type { BrowserEntry, BrowserFlag, BrowserSession, QueueItem, WorkflowBinding } from './types';
import type { ParameterValue, WorkflowParameter } from '../editor/types';
import { BatchController } from '../batch/controller';
import type { BatchSessionReference } from '../batch/types';
import type { BatchWorkflowContext } from '../batch/model';
import { BatchPanel } from '../components/BatchPanel';
import { createBatchPlatform } from '../platform/batch';
import { createImageSet } from '../imageset/model';
import type { ImageSetCollection, ImageSetOrder } from '../imageset/model';
import { Icon } from '../ui/Icon';

export interface BrowserQueueProps {
  mode?: 'browse' | 'batch';
  /**
   * Whether the browse surface is the one on screen. The component stays mounted
   * (and hidden) in the other workspace modes, so its window-level key handlers
   * have to be inert there: rating and flag shortcuts otherwise write marks to
   * disk while the user is editing the graph.
   */
  active?: boolean;
  platform?: BrowserPlatform;
  initialFolder?: string;
  workflowBinding?: WorkflowBinding | null;
  workflowParameters?: WorkflowParameter[];
  onPromoteOverrides?: (overrides: Record<string, ParameterValue>) => void | Promise<void>;
  unsavedWorkflowWorkingCopy?: string | null;
  viewerTargets?: BrowserSession['viewer']['targets'];
  panelLayout?: string;
  onSessionLoaded?: (session: BrowserSession) => void | Promise<void>;
  onOpenImage?: (path: string) => void | Promise<void>;
  imageSets?: ImageSetCollection[];
  activeImageSetId?: string | null;
  onImageSetsChange?: (collections: ImageSetCollection[], activeId: string | null) => void;
  onOpenImageSet?: (paths: string[], order: ImageSetOrder) => void | Promise<void>;
  batchPlatform?: import('../batch/types').BatchPlatform;
  batchWorkflow?: BatchWorkflowContext | null;
  onOpenFailedItem?: (item: import('../batch/types').BatchItem) => void | Promise<void>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function metadataValue(value: string | number | null | undefined): string {
  return value === null || value === undefined || value === '' ? '—' : String(value);
}

export function createBrowserSession(
  browser: BrowserState,
  queue: QueueState,
  options: {
    workflowBinding?: WorkflowBinding | null;
    unsavedWorkflowWorkingCopy?: string | null;
    viewerTargets?: BrowserSession['viewer']['targets'];
    batchReference?: BatchSessionReference;
    imageSets?: ImageSetCollection[];
    activeImageSetId?: string | null;
    panelLayout?: string;
  },
): BrowserSession {
  return {
    version: 1,
    browser: {
      currentFolder: browser.currentFolder,
      view: browser.view,
      selectedPaths: browser.selectedPaths,
    },
    queue: {
      items: queue.items,
      currentPath: queue.currentPath,
      selectedPaths: queue.selectedPaths,
    },
    testSet: { currentPath: queue.testSetCurrentPath },
    workflow: {
      selected: options.workflowBinding ?? null,
      unsavedWorkingCopy: options.unsavedWorkflowWorkingCopy ?? null,
    },
    viewer: { targets: options.viewerTargets ?? { A: null, B: null } },
    batch: options.batchReference ?? { jobId: null, statePath: null },
    imageSets: options.imageSets ?? [],
    activeImageSetId: options.activeImageSetId ?? null,
    panelLayout: options.panelLayout ?? 'default',
  };
}

export function BrowserQueue({
  mode = 'browse',
  active = true,
  platform: providedPlatform,
  initialFolder = '',
  workflowBinding = null,
  workflowParameters = [],
  onPromoteOverrides,
  unsavedWorkflowWorkingCopy = null,
  viewerTargets,
  panelLayout = 'default',
  onSessionLoaded,
  onOpenImage,
  imageSets: providedImageSets,
  activeImageSetId: providedActiveImageSetId,
  onImageSetsChange,
  onOpenImageSet,
  batchPlatform: providedBatchPlatform,
  batchWorkflow = null,
  onOpenFailedItem,
}: BrowserQueueProps) {
  const [platform] = useState<BrowserPlatform>(() => providedPlatform ?? createBrowserPlatform());
  const [browserController] = useState(() => new BrowserController(platform));
  const [queueController] = useState(() => new QueueController());
  const [batchController] = useState(() => new BatchController(providedBatchPlatform ?? createBatchPlatform()));
  const [browser, setBrowser] = useState(browserController.state);
  const [queue, setQueue] = useState(queueController.state);
  const [, setBatchRevision] = useState(0);
  const [batchReference, setBatchReference] = useState<BatchSessionReference>(batchController.sessionReference);
  const [localImageSets, setLocalImageSets] = useState<ImageSetCollection[]>([]);
  const [localActiveImageSetId, setLocalActiveImageSetId] = useState<string | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [queueDropActive, setQueueDropActive] = useState(false);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const imageSets = providedImageSets ?? localImageSets;
  const activeImageSetId = providedActiveImageSetId === undefined ? localActiveImageSetId : providedActiveImageSetId;

  useEffect(() => {
    const unsubscribeBrowser = browserController.subscribe(setBrowser);
    const unsubscribeQueue = queueController.subscribe(setQueue);
    setBrowser(browserController.state);
    setQueue(queueController.state);
    return () => {
      unsubscribeBrowser();
      unsubscribeQueue();
    };
  }, [browserController, queueController]);

  useEffect(() => batchController.subscribe(() => {
    setBatchRevision((revision) => revision + 1);
    setBatchReference(batchController.sessionReference);
  }), [batchController]);

  useEffect(() => () => batchController.dispose(), [batchController]);

  useEffect(() => {
    let cancelled = false;
    const initialize = async () => {
      try {
        const saved = await platform.loadSession();
        if (cancelled) return;
        const folder = saved?.browser.currentFolder || initialFolder;
        if (saved) {
          await onSessionLoaded?.(saved);
          if (providedImageSets === undefined) {
            setLocalImageSets(saved.imageSets);
            setLocalActiveImageSetId(saved.activeImageSetId);
          }
          browserController.setView(saved.browser.view);
          queueController.restore({
            items: saved.queue.items,
            currentPath: saved.queue.currentPath,
            selectedPaths: saved.queue.selectedPaths,
            testSetCurrentPath: saved.testSet.currentPath,
          });
          setBatchReference(saved.batch ?? { jobId: null, statePath: null });
          if (saved.batch?.statePath) void batchController.load(saved.batch.statePath).catch(() => undefined);
        }
        if (folder) {
          await browserController.loadFolder(folder);
          if (saved && !cancelled) {
            browserController.selectAllVisible(saved.browser.selectedPaths);
          }
        }
      } catch (caught) {
        if (!cancelled) setError(errorMessage(caught));
      } finally {
        if (!cancelled) setReady(true);
      }
    };
    void initialize();
    return () => {
      cancelled = true;
    };
  // Restoring the session mutates these parent props. Re-running would reload
  // the workflow and clear the image source after it has been opened.
  }, [batchController, browserController, platform, queueController]);

  useEffect(() => {
    if (!ready) return;
    const timeout = window.setTimeout(() => {
      void platform.saveSession(createBrowserSession(browser, queue, {
        workflowBinding,
        unsavedWorkflowWorkingCopy,
        viewerTargets,
        batchReference,
        imageSets,
        activeImageSetId,
        panelLayout,
      })).catch((caught) => {
        setError(errorMessage(caught));
      });
    }, 150);
    return () => window.clearTimeout(timeout);
  }, [activeImageSetId, batchReference, browser, imageSets, panelLayout, platform, queue, ready, unsavedWorkflowWorkingCopy, viewerTargets, workflowBinding]);

  const visibleEntries = useMemo(
    () => sortBrowserEntries(
      filterBrowserEntries(browser.entries, browser.view.filter),
      browser.view.sort,
    ),
    [browser.entries, browser.view.filter, browser.view.sort],
  );
  const previewEntry = useMemo(() => {
    const path = previewPath ?? queue.currentPath ?? browser.selectedPaths.at(-1) ?? null;
    return path ? browser.entries.find((entry) => entry.path === path)
      ?? queue.items.find((item) => item.path === path)?.source
      ?? null : null;
  }, [browser.entries, browser.selectedPaths, previewPath, queue.currentPath, queue.items]);
  const currentQueueItem = useMemo<QueueItem | null>(
    () => queue.items.find((item) => item.path === queue.currentPath) ?? null,
    [queue.currentPath, queue.items],
  );
  const currentOverrideCount = currentQueueItem ? Object.keys(currentQueueItem.overrides).length : 0;
  const testSetCount = queue.items.filter((item) => item.testSet).length;
  const breadcrumbs = breadcrumbSegments(browser.currentFolder);
  const browserLayout = browser.view.layout ?? 'grid';

  const run = useCallback(async (action: () => Promise<void>) => {
    try {
      setError(null);
      await action();
    } catch (caught) {
      setError(errorMessage(caught));
    }
  }, []);

  const openPreview = useCallback((path: string) => {
    setPreviewPath(path);
    queueController.choose(path);
    if (onOpenImage) void run(() => Promise.resolve(onOpenImage(path)));
  }, [onOpenImage, queueController, run]);

  const openFolder = useCallback((path: string) => {
    void run(() => browserController.loadFolder(path));
  }, [browserController, run]);

  const chooseFolder = useCallback(() => {
    void run(async () => {
      const path = await platform.chooseFolder();
      if (path) await browserController.loadFolder(path);
    });
  }, [browserController, platform, run]);

  const selectEntry = useCallback((event: React.MouseEvent, entry: BrowserEntry) => {
    if (entry.kind === 'directory') {
      openFolder(entry.path);
      return;
    }
    const additive = event.metaKey || event.ctrlKey || event.shiftKey;
    browserController.toggleSelection(entry.path, additive);
    openPreview(entry.path);
  }, [browserController, openFolder, openPreview]);

  const addSelection = useCallback(() => {
    const selected = browser.entries.filter((entry) => browser.selectedPaths.includes(entry.path));
    queueController.addSelection(selected, workflowBinding);
  }, [browser.entries, browser.selectedPaths, queueController, workflowBinding]);

  const commitImageSets = useCallback((collections: ImageSetCollection[], activeId: string | null) => {
    if (onImageSetsChange) onImageSetsChange(collections, activeId);
    if (providedImageSets === undefined) {
      setLocalImageSets(collections);
      setLocalActiveImageSetId(activeId);
    }
  }, [onImageSetsChange, providedImageSets]);

  const openImageSetCollection = useCallback((collection: ImageSetCollection) => {
    commitImageSets(imageSets, collection.id);
    if (!onOpenImageSet) return;
    void run(() => Promise.resolve(onOpenImageSet(collection.members.map((member) => member.path), collection.order)));
  }, [commitImageSets, imageSets, onOpenImageSet, run]);

  const createImageSetFromEntries = useCallback((entries: BrowserEntry[], order: ImageSetOrder) => {
    void run(async () => {
      const collection = createImageSet(entries, order, `Image Set ${imageSets.length + 1}`);
      const next = [...imageSets.filter((candidate) => candidate.id !== collection.id), collection];
      commitImageSets(next, collection.id);
      await onOpenImageSet?.(collection.members.map((member) => member.path), collection.order);
    });
  }, [commitImageSets, imageSets, onOpenImageSet, run]);

  const browserSelection = useMemo(
    () => browser.entries.filter((entry) => browser.selectedPaths.includes(entry.path) && entry.kind === 'file'),
    [browser.entries, browser.selectedPaths],
  );
  const queueSelection = useMemo(
    () => queue.items.filter((item) => queue.selectedPaths.includes(item.path)).map((item) => item.source),
    [queue.items, queue.selectedPaths],
  );

  const removeImageSet = useCallback((collection: ImageSetCollection) => {
    const next = imageSets.filter((candidate) => candidate.id !== collection.id);
    commitImageSets(next, activeImageSetId === collection.id ? next[0]?.id ?? null : activeImageSetId);
  }, [activeImageSetId, commitImageSets, imageSets]);

  const mark = useCallback((path: string, rating: number | null, flag: BrowserFlag) => {
    void run(async () => {
      await browserController.mark(path, rating, flag);
      queueController.updateSourceMarks(path, rating, flag);
    });
  }, [browserController, queueController, run]);

  useEffect(() => {
    if (!active) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (typeof target?.closest === 'function' && target.closest('input, textarea, select, [contenteditable="true"]')) return;
      const path = previewPath ?? queue.currentPath ?? browser.selectedPaths.at(-1);
      if (!path) return;
      const entry = browser.entries.find((candidate) => candidate.path === path);
      if (!entry || entry.kind !== 'file') return;
      if (/^[0-5]$/.test(event.key)) {
        event.preventDefault();
        const rating = Number(event.key);
        mark(path, rating === 0 ? null : rating, entry.flag);
      } else if (event.key.toLowerCase() === 'p') {
        event.preventDefault();
        mark(path, entry.rating, entry.flag === 'pick' ? 'none' : 'pick');
      } else if (event.key.toLowerCase() === 'x') {
        event.preventDefault();
        mark(path, entry.rating, entry.flag === 'reject' ? 'none' : 'reject');
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [active, browser.entries, browser.selectedPaths, mark, previewPath, queue.currentPath]);

  const toggleTestSet = useCallback((path: string, included: boolean) => {
    queueController.setTestSet([path], included);
  }, [queueController]);

  const moveTest = useCallback((direction: 'previous' | 'next') => {
    const item = queueController.moveTest(direction);
    if (item) openPreview(item.path);
  }, [openPreview, queueController]);

  const setOverride = useCallback((parameter: WorkflowParameter, value: ParameterValue) => {
    if (currentQueueItem) queueController.setOverride(currentQueueItem.path, parameter.id, value);
  }, [currentQueueItem, queueController]);

  const resetParameterOverride = useCallback((parameterId: string) => {
    if (currentQueueItem) queueController.resetOverride(currentQueueItem.path, parameterId);
  }, [currentQueueItem, queueController]);

  const promoteCurrentOverrides = useCallback(() => {
    if (!currentQueueItem || !onPromoteOverrides || currentOverrideCount === 0) return;
    const overrides = queueController.promote(currentQueueItem.path);
    void run(() => Promise.resolve(onPromoteOverrides(overrides)));
  }, [currentOverrideCount, currentQueueItem, onPromoteOverrides, queueController, run]);

  return (
    <section aria-label="File browser and working queue" className={`browser-queue browser-queue--${mode}`}>
      <header className="browser-queue__toolbar">
        <div className="browser-queue__title">
          <span className="eyebrow">{mode === 'batch' ? 'Batch processing' : 'Browse → Queue'}</span>
          <strong>{mode === 'batch' ? `${queue.items.length} queued image${queue.items.length === 1 ? '' : 's'}` : browser.currentFolder || 'Choose a photo folder'}</strong>
          {browser.loading && <small>Loading thumbnails and metadata…</small>}
        </div>
        <nav aria-label="Folder breadcrumbs" className="browser-breadcrumbs">
          {breadcrumbs.map((breadcrumb, index) => (
            <span key={breadcrumb.path}>
              <button
                className={index === breadcrumbs.length - 1 ? 'is-current' : ''}
                disabled={index === breadcrumbs.length - 1}
                onClick={() => openFolder(breadcrumb.path)}
                type="button"
              >
                {breadcrumb.name}
              </button>
              {index < breadcrumbs.length - 1 && <span aria-hidden="true"> / </span>}
            </span>
          ))}
        </nav>
        <div className="browser-queue__actions">
          <button aria-label="Choose folder" className="button button--quiet" onClick={chooseFolder} type="button">
            Choose folder
          </button>
          {browser.nextOffset !== null && (
            <button className="button button--quiet" disabled={browser.loading} onClick={() => void run(() => browserController.loadMore())} type="button">
              Load more
            </button>
          )}
        </div>
      </header>

      <div className="browser-queue__body">
        <section aria-label="File browser" className="browser-panel">
          <div className="browser-panel__controls">
            <label className="browser-search">
              <span className="sr-only">Filter files</span>
              <input
                aria-label="Filter files"
                onChange={(event) => browserController.setView({ filter: { ...browser.view.filter, query: event.target.value } })}
                placeholder="Filter files…"
                value={browser.view.filter.query}
              />
            </label>
            <label>
              <span className="sr-only">Sort files</span>
              <select aria-label="Sort files" onChange={(event) => browserController.setView({ sort: { ...browser.view.sort, by: event.target.value as BrowserState['view']['sort']['by'] } })} value={browser.view.sort.by}>
                <option value="name">Name</option>
                <option value="modified">Modified</option>
                <option value="size">Size</option>
                <option value="rating">Rating</option>
              </select>
            </label>
            <label>
              <span className="sr-only">Filter rating</span>
              <select aria-label="Filter rating" onChange={(event) => browserController.setView({ filter: { ...browser.view.filter, rating: event.target.value as BrowserState['view']['filter']['rating'] } })} value={browser.view.filter.rating}>
                <option value="any">Any rating</option>
                <option value="rated">Rated</option>
                <option value="unrated">Unrated</option>
              </select>
            </label>
            <label>
              <span className="sr-only">Filter flag</span>
              <select aria-label="Filter flag" onChange={(event) => browserController.setView({ filter: { ...browser.view.filter, flag: event.target.value as BrowserState['view']['filter']['flag'] } })} value={browser.view.filter.flag}>
                <option value="any">Any flag</option>
                <option value="pick">Picks</option>
                <option value="reject">Rejects</option>
                <option value="none">Unflagged</option>
              </select>
            </label>
            <button
              aria-label={`Sort ${browser.view.sort.direction === 'asc' ? 'descending' : 'ascending'}`}
              className="button button--quiet"
              onClick={() => browserController.setView({ sort: { ...browser.view.sort, direction: browser.view.sort.direction === 'asc' ? 'desc' : 'asc' } })}
              type="button"
            >
              {browser.view.sort.direction === 'asc' ? <Icon name="arrowUp" /> : <Icon name="arrowDown" />}
            </button>
            <div aria-label="Browser view" className="browser-view-toggle" role="group">
              <button aria-label="Grid view" aria-pressed={browserLayout === 'grid'} className={browserLayout === 'grid' ? 'is-active' : ''} onClick={() => browserController.setView({ layout: 'grid' })} type="button">Grid</button>
              <button aria-label="List view" aria-pressed={browserLayout === 'list'} className={browserLayout === 'list' ? 'is-active' : ''} onClick={() => browserController.setView({ layout: 'list' })} type="button">List</button>
            </div>
            <label>
              <span className="sr-only">Thumbnail size</span>
              <select aria-label="Thumbnail size" onChange={(event) => browserController.setView({ thumbnailSize: event.target.value as BrowserState['view']['thumbnailSize'] })} value={browser.view.thumbnailSize}>
                <option value="small">Small</option>
                <option value="medium">Medium</option>
                <option value="large">Large</option>
              </select>
            </label>
            <button aria-label="Add selected to queue" className="button button--primary" disabled={browser.selectedPaths.length === 0} onClick={addSelection} type="button">
              Add {browser.selectedPaths.length || ''} to queue
            </button>
            <button
              aria-label="Create ordered ImageSet from browser selection"
              className="button button--quiet"
              disabled={browserSelection.length === 0}
              onClick={() => createImageSetFromEntries(browserSelection, 'ordered')}
              type="button"
            >
              Set ordered
            </button>
            <button
              aria-label="Create unordered ImageSet from browser selection"
              className="button button--quiet"
              disabled={browserSelection.length === 0}
              onClick={() => createImageSetFromEntries(browserSelection, 'unordered')}
              type="button"
            >
              Set unordered
            </button>
          </div>
          {browser.error && <p className="browser-error">{browser.error}</p>}
          <div className={`browser-grid browser-grid--${browserLayout} browser-grid--${browser.view.thumbnailSize}`}>
            {!browser.currentFolder && <p className="empty-state">Choose a folder to browse photographs.</p>}
            {browser.currentFolder && browser.entries.length === 0 && <p className="empty-state">Folder is empty</p>}
            {browser.currentFolder && browser.entries.length > 0 && visibleEntries.length === 0 && <p className="empty-state">No files match the current filter</p>}
            {visibleEntries.map((entry) => {
              const selected = browser.selectedPaths.includes(entry.path);
              return (
                <article className={`browser-tile${selected ? ' is-selected' : ''}`} key={entry.path}>
                  <button
                    aria-label={entry.kind === 'directory' ? `Open folder ${entry.name}` : `Select ${entry.name}`}
                    aria-pressed={entry.kind === 'file' ? selected : undefined}
                    className="browser-tile__open"
                    draggable={entry.kind === 'file'}
                    onClick={(event) => selectEntry(event, entry)}
                    onDragStart={(event) => {
                      if (entry.kind === 'file') {
                        event.dataTransfer.effectAllowed = 'copy';
                        event.dataTransfer.setData('text/rawweave-path', entry.path);
                      }
                    }}
                    onDragEnd={() => setQueueDropActive(false)}
                    type="button"
                  >
                    {entry.thumbnail ? <img alt="" src={entry.thumbnail} /> : <span className="browser-tile__placeholder">{entry.kind === 'directory' ? '▰' : '◌'}</span>}
                    <strong>{entry.name}</strong>
                    {entry.kind === 'file' && <small>{entry.metadata ? `${entry.metadata.width} × ${entry.metadata.height}` : 'Metadata pending'}</small>}
                  </button>
                  {entry.kind === 'file' && (
                    <div aria-label={`Marks for ${entry.name}`} className="browser-tile__marks">
                      <div aria-label={`Rating for ${entry.name}`} className="browser-tile__rating" role="group">
                        {[0, 1, 2, 3, 4, 5].map((rating) => {
                          const activeRating = rating === 0 ? null : rating;
                          const active = entry.rating === activeRating;
                          return (
                            <button
                              aria-label={`Set ${rating} stars for ${entry.name}`}
                              aria-pressed={active}
                              className={active ? 'is-active' : ''}
                              key={rating}
                              onClick={() => mark(entry.path, active ? null : activeRating, entry.flag)}
                              title={`${rating} star${rating === 1 ? '' : 's'}`}
                              type="button"
                            >
                              {rating}
                            </button>
                          );
                        })}
                      </div>
                      <div aria-label={`Flags for ${entry.name}`} className="browser-tile__flags" role="group">
                        <button aria-label={`Mark ${entry.name} as pick`} aria-pressed={entry.flag === 'pick'} className={entry.flag === 'pick' ? 'is-active' : ''} onClick={() => mark(entry.path, entry.rating, entry.flag === 'pick' ? 'none' : 'pick')} type="button">Pick</button>
                        <button aria-label={`Mark ${entry.name} as reject`} aria-pressed={entry.flag === 'reject'} className={entry.flag === 'reject' ? 'is-active is-reject' : ''} onClick={() => mark(entry.path, entry.rating, entry.flag === 'reject' ? 'none' : 'reject')} type="button">Reject</button>
                      </div>
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        </section>

        <aside
          aria-label="Working Queue"
          className={`queue-panel${queueDropActive ? ' queue-panel--drop-target' : ''}`}
          onDragEnter={(event) => {
            if (event.dataTransfer.types.includes('text/rawweave-path')) setQueueDropActive(true);
          }}
          onDragLeave={() => setQueueDropActive(false)}
          onDragOver={(event) => {
            event.preventDefault();
            if (event.dataTransfer.types.includes('text/rawweave-path')) {
              event.dataTransfer.dropEffect = 'copy';
              setQueueDropActive(true);
            }
          }}
          onDrop={(event) => {
            event.preventDefault();
            setQueueDropActive(false);
            const path = event.dataTransfer.getData('text/rawweave-path');
            const entry = browser.entries.find((candidate) => candidate.path === path && candidate.kind === 'file');
            if (entry) {
              queueController.addSelection([entry], workflowBinding);
              openPreview(entry.path);
            }
          }}
        >
          <header className="queue-panel__heading">
            <div>
              <span className="eyebrow">Working Queue</span>
              <strong>{queue.items.length} queued</strong>
            </div>
            <button className="button button--quiet" disabled={queue.items.length === 0} onClick={() => queueController.clear()} type="button">Clear</button>
          </header>
          <div className="queue-list">
            {queue.items.length === 0 && <p className="empty-state">Select photos in the browser, then add them here.</p>}
            {queue.items.map((item) => {
              const overrideCount = Object.keys(item.overrides).length;
              return (
              <div className={`queue-item${queue.currentPath === item.path ? ' is-current' : ''}`} key={item.path}>
                <button aria-label={`Preview ${item.name}`} className="queue-item__preview" onClick={() => openPreview(item.path)} type="button">
                  {item.source.thumbnail ? <img alt="" src={item.source.thumbnail} /> : <span>◌</span>}
                  <span><strong>{item.name}</strong><small>{item.processingStatus} · {overrideCount ? `${overrideCount} override${overrideCount === 1 ? '' : 's'}` : 'workflow default'}</small></span>
                </button>
                <label className="queue-item__select">
                  <input
                    aria-label={`Select ${item.name} for override copy`}
                    checked={queue.selectedPaths.includes(item.path)}
                    onChange={(event) => queueController.select(event.target.checked
                      ? [...queue.selectedPaths, item.path]
                      : queue.selectedPaths.filter((path) => path !== item.path))}
                    type="checkbox"
                  />
                  Copy
                </label>
                <label className="queue-item__test">
                  <input aria-label={`Include ${item.name} in test set`} checked={item.testSet} onChange={(event) => toggleTestSet(item.path, event.target.checked)} type="checkbox" />
                  Test
                </label>
                <button aria-label={`Remove ${item.name} from queue`} className="icon-button" onClick={() => queueController.remove([item.path])} type="button"><Icon name="close" /></button>
              </div>
              );
            })}
          </div>
          <section aria-label="Image Sets" className="imageset-panel">
            <header className="imageset-panel__heading">
              <div>
                <span className="eyebrow">ImageSets</span>
                <strong>{imageSets.length} collection{imageSets.length === 1 ? '' : 's'}</strong>
              </div>
              <div className="imageset-panel__actions">
                <button
                  aria-label="Create ordered ImageSet from queue selection"
                  className="button button--quiet"
                  disabled={queueSelection.length === 0}
                  onClick={() => createImageSetFromEntries(queueSelection, 'ordered')}
                  type="button"
                >
                  Queue → Set
                </button>
                <button
                  aria-label="Create unordered ImageSet from queue selection"
                  className="button button--quiet"
                  disabled={queueSelection.length === 0}
                  onClick={() => createImageSetFromEntries(queueSelection, 'unordered')}
                  type="button"
                >
                  Unordered
                </button>
              </div>
            </header>
            <div className="imageset-list">
              {imageSets.length === 0 && <p className="empty-state empty-state--compact">Select browser files or queue items to create a collection.</p>}
              {imageSets.map((collection) => (
                <article className={`imageset-card${collection.id === activeImageSetId ? ' is-active' : ''}`} key={collection.id}>
                  <button
                    aria-label={`Select Image Set ${collection.name}`}
                    className="imageset-card__select"
                    onClick={() => commitImageSets(imageSets, collection.id)}
                    type="button"
                  >
                    <strong>{collection.name}</strong>
                    <small>{collection.order} · {collection.members.length} members</small>
                    <span>{collection.members.map((member) => member.name).join(' · ')}</span>
                  </button>
                  <div className="imageset-card__actions">
                    <button aria-label={`Open Image Set ${collection.name}`} className="button button--quiet" onClick={() => openImageSetCollection(collection)} type="button">Open</button>
                    <button aria-label={`Remove Image Set ${collection.name}`} className="icon-button" onClick={() => removeImageSet(collection)} type="button"><Icon name="close" /></button>
                  </div>
                </article>
              ))}
            </div>
          </section>
          <section aria-label="Per-image workflow overrides" className="queue-overrides">
            <header className="queue-overrides__heading">
              <div>
                <span className="eyebrow">Per-image overrides</span>
                <strong>{currentQueueItem?.name ?? 'Select a queue item'}</strong>
              </div>
              <span className={currentOverrideCount > 0 ? 'override-indicator is-active' : 'override-indicator'}>
                {currentOverrideCount > 0 ? `${currentOverrideCount} override${currentOverrideCount === 1 ? '' : 's'}` : 'workflow default'}
              </span>
            </header>
            <div className="queue-overrides__actions">
              <button
                aria-label="Copy overrides to selected"
                className="button button--quiet"
                disabled={!currentQueueItem || queue.selectedPaths.length === 0}
                onClick={() => queueController.copyToSelected(currentQueueItem?.path)}
                type="button"
              >
                Copy to selected
              </button>
              <button
                aria-label="Apply overrides to all"
                className="button button--quiet"
                disabled={!currentQueueItem}
                onClick={() => queueController.applyToAll(currentQueueItem?.path)}
                type="button"
              >
                Apply all
              </button>
              <button
                aria-label="Promote overrides to workflow default"
                className="button button--quiet"
                disabled={!currentQueueItem || currentOverrideCount === 0 || !onPromoteOverrides}
                onClick={promoteCurrentOverrides}
                type="button"
              >
                Promote to default
              </button>
            </div>
            {workflowParameters.length === 0 && (
              <p className="empty-state empty-state--compact">Expose workflow parameters to edit them per image.</p>
            )}
            {currentQueueItem && workflowParameters.length > 0 && (
              <div className="queue-overrides__parameters">
                {workflowParameters.map((parameter) => {
                  const overridden = Object.prototype.hasOwnProperty.call(currentQueueItem.overrides, parameter.id);
                  const value = currentQueueItem.overrides[parameter.id] ?? parameter.default;
                  return (
                    <div className={`queue-override${overridden ? ' is-overridden' : ''}`} key={parameter.id}>
                      <label className="queue-override__field">
                        <span>{parameter.name}</span>
                        {parameter.parameterType === 'Boolean' ? (
                          <input
                            aria-label={`Override ${parameter.name}`}
                            checked={Boolean(value)}
                            onChange={(event) => setOverride(parameter, event.target.checked)}
                            type="checkbox"
                          />
                        ) : (
                          <input
                            aria-label={`Override ${parameter.name}`}
                            onChange={(event) => {
                              const next = parameter.parameterType === 'Float' || parameter.parameterType === 'Integer'
                                ? Number(event.target.value)
                                : event.target.value;
                              if (parameter.parameterType === 'String' || Number.isFinite(next)) setOverride(parameter, next);
                            }}
                            step={parameter.parameterType === 'Float' ? 0.01 : parameter.parameterType === 'Integer' ? 1 : undefined}
                            type={parameter.parameterType === 'Float' || parameter.parameterType === 'Integer' ? 'number' : 'text'}
                            value={String(value)}
                          />
                        )}
                      </label>
                      <button
                        aria-label={`Reset ${parameter.name} override`}
                        className="button button--quiet"
                        disabled={!overridden}
                        onClick={() => resetParameterOverride(parameter.id)}
                        type="button"
                      >
                        Reset
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </section>
          <div className="queue-panel__testset">
            <div>
              <span className="eyebrow">Test Set · {testSetCount}</span>
              <strong>{queue.testSetCurrentPath ? queue.items.find((item) => item.path === queue.testSetCurrentPath)?.name : 'Quick compare'}</strong>
            </div>
            <div className="queue-panel__testset-actions">
              <button aria-label="Previous test item" disabled={testSetCount === 0} onClick={() => moveTest('previous')} type="button">←</button>
              <button aria-label="Next test item" disabled={testSetCount === 0} onClick={() => moveTest('next')} type="button">→</button>
            </div>
          </div>
          <div aria-label="Selected image preview" className="browser-preview">
            {previewEntry?.thumbnail ? <img alt={`Preview ${previewEntry.name}`} src={previewEntry.thumbnail} /> : <span>{previewEntry ? 'Preview pending' : 'Select an image to preview'}</span>}
            {previewEntry?.metadata && (
              <dl>
                <div><dt>Camera</dt><dd>{metadataValue(previewEntry.metadata.camera)}</dd></div>
                <div><dt>Lens</dt><dd>{metadataValue(previewEntry.metadata.lens)}</dd></div>
                <div><dt>ISO</dt><dd>{metadataValue(previewEntry.metadata.iso)}</dd></div>
                <div><dt>Exposure</dt><dd>{previewEntry.metadata.shutter === null || previewEntry.metadata.shutter === undefined ? '—' : `${previewEntry.metadata.shutter}s`}</dd></div>
              </dl>
            )}
          </div>
        </aside>
      </div>
      <BatchPanel
        controller={batchController}
        currentPath={queue.currentPath}
        onOpenItem={(item) => {
          void run(async () => {
            openPreview(item.sourcePath);
            await onOpenFailedItem?.(item);
          });
        }}
        queueItems={queue.items}
        workflow={batchWorkflow}
      />
      {error && <p className="browser-queue__error" role="alert">{error}</p>}
    </section>
  );
}
