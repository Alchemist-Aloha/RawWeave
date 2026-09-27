import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type RefObject } from 'react';
import type { EditorNode, ParameterValue, SourceResult } from '../editor/types';
import { MaskPainter } from '../mask/MaskPainter';
import { Scopes } from './Scopes';
import { analyzeImageElement, drawClippingOverlay, type ImageAnalysis } from '../viewer/analysis';
import { ViewerController } from '../viewer/controller';
import type { PreviewTarget, ViewerComparison, ViewerId, ViewerPaneState } from '../viewer/types';
import { describeOperationError } from '../ui/errors';
import { Icon } from '../ui/Icon';

interface ViewerProps {
  controller: ViewerController;
  nodes: EditorNode[];
  revision: number;
  source: SourceResult | null;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
  docked?: boolean;
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
}

const COMPARISON_BUTTONS: Array<{ id: ViewerComparison; label: string; short: string }> = [
  { id: 'side-by-side', label: 'Compare A and B', short: 'A/B' },
  { id: 'wipe', label: 'Wipe', short: 'Wipe' },
  { id: 'blink', label: 'Blink', short: 'Blink' },
  { id: 'difference', label: 'Difference', short: 'Diff' },
];

/**
 * The image transform, in one place.
 *
 * Absolute positioning pins the image to the stage centre so its intrinsic size
 * never inflates the grid track; the centring translate then scales it about that
 * same centre for fit/zoom/pan.
 */
export function imageTransform(pan: { x: number; y: number }, displayScale: number): string {
  return `translate(-50%, -50%) translate(${pan.x}px, ${pan.y}px) scale(${displayScale})`;
}

const PREVIEWABLE_DATA_TYPES = new Set([
  'core.Image',
  'core.Mask',
  'core.MaskSet',
  'core.LabelMap',
  'core.ConfidenceMap',
  'core.DepthMap',
  'core.RegionSet',
  'color.DisplayRGB',
  'color.SceneLinearRGB',
]);

export function targetsFor(nodes: EditorNode[]): PreviewTarget[] {
  return nodes.flatMap((node) =>
    node.descriptor.outputs
      .filter((output) => PREVIEWABLE_DATA_TYPES.has(output.dataType))
      .map((output) => ({
        nodeId: node.id,
        nodeName: node.descriptor.name,
        outputPort: output.id,
        outputName: output.name,
        dataType: output.dataType,
      })),
  );
}

function targetKey(target: PreviewTarget): string {
  return `${target.nodeId}:${target.outputPort}`;
}

export interface ClippingOverlayProps {
  analysis: ImageAnalysis | null;
  enabled: boolean;
  imageRef: RefObject<HTMLImageElement | null>;
  stageRef: RefObject<HTMLElement | null>;
  viewer: ViewerId;
  /** Set by the overlay to the geometry-only update the pan/zoom path calls. */
  handle?: RefObject<ClippingOverlayHandle>;
}

/**
 * Lets the viewer move an already-drawn overlay with the image.
 *
 * Panning and zooming only translate the image, so the overlay has to follow the
 * new bounds without redrawing its pixel raster every frame.
 */
export interface ClippingOverlayHandle {
  reposition: (() => void) | null;
}

export function ClippingOverlay({ analysis, enabled, imageRef, stageRef, viewer, handle }: ClippingOverlayProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useLayoutEffect(() => {
    if (!enabled || !analysis) return;
    const canvas = canvasRef.current;
    const image = imageRef.current;
    const stage = stageRef.current;
    if (!canvas || !image || !stage) return;

    const reposition = () => {
      const stageBounds = stage.getBoundingClientRect();
      const imageBounds = image.getBoundingClientRect();
      canvas.style.left = `${imageBounds.left - stageBounds.left}px`;
      canvas.style.top = `${imageBounds.top - stageBounds.top}px`;
      canvas.style.width = `${imageBounds.width}px`;
      canvas.style.height = `${imageBounds.height}px`;
    };
    const update = () => {
      reposition();
      drawClippingOverlay(analysis, canvas);
    };
    if (handle) handle.current.reposition = reposition;

    update();
    window.addEventListener('resize', update);
    if (typeof ResizeObserver === 'undefined') {
      return () => {
        if (handle) handle.current.reposition = null;
        window.removeEventListener('resize', update);
      };
    }
    let frame = 0;
    const schedule = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(update);
    };
    const observer = new ResizeObserver(schedule);
    observer.observe(stage);
    observer.observe(image);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      window.removeEventListener('resize', update);
      if (handle) handle.current.reposition = null;
    };
  }, [analysis, enabled, handle, imageRef, stageRef]);

  if (!enabled || !analysis) return null;
  return (
    <canvas
      aria-label={`Viewer ${viewer} clipping overlay`}
      className="viewer-pane__clipping-overlay"
      ref={canvasRef}
      role="img"
    />
  );
}

function TargetSelect({ viewer, pane, options, controller }: {
  viewer: ViewerId;
  pane: ViewerPaneState;
  options: PreviewTarget[];
  controller: ViewerController;
}) {
  const selectedKey = pane.target ? targetKey(pane.target) : '';
  return (
    <select
      aria-label={`Viewer ${viewer} target`}
      className="viewer-pane__target"
      onChange={(event) => {
        const next = options.find((option) => targetKey(option) === event.target.value) ?? null;
        controller.setTarget(viewer, next);
      }}
      value={selectedKey}
    >
      <option value="">Select image output</option>
      {options.map((option) => (
        <option key={targetKey(option)} value={targetKey(option)}>
          {option.nodeName} · {option.outputName}
        </option>
      ))}
    </select>
  );
}

function Pane({ viewer, pane, options, controller, paintedNode, onPaintedMaskChange, onAnalysis, analysis = null, surface = false, clippingOverlay = false, className = '', style }: {
  viewer: ViewerId;
  pane: ViewerPaneState;
  options: PreviewTarget[];
  controller: ViewerController;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
  onAnalysis?: (analysis: ImageAnalysis | null) => void;
  analysis?: ImageAnalysis | null;
  surface?: boolean;
  clippingOverlay?: boolean;
  className?: string;
  style?: CSSProperties;
}) {
  const stageRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  /** Live pan while a gesture is in flight; the controller only sees the commit. */
  const livePan = useRef(pane.pan);
  const dragOrigin = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const panFrame = useRef(0);
  const overlayHandle = useRef<ClippingOverlayHandle>({ reposition: null });
  const imageUrl = pane.imageUrl;
  const errorNotice = pane.status === 'error' && pane.error
    ? describeOperationError(pane.error, {
      nodeId: pane.target?.nodeId,
      nodeLabel: pane.target?.nodeName,
      operation: 'viewer-render',
      outputPort: pane.target?.outputPort,
    })
    : null;

  /**
   * Writes the image transform straight to the DOM.
   *
   * Panning used to publish editor state on every pointer move, which re-rendered
   * the whole viewer (target selects, scope canvases, mask controls) and left the
   * image trailing the cursor whenever that render was slow. Writing the transform
   * here keeps the drag tied to the pointer; React owns it again once the gesture
   * commits. `will-change` is scoped to the gesture so a resting image is not a
   * permanent compositing layer.
   */
  const writeTransform = useCallback((pan: { x: number; y: number }, panning: boolean) => {
    const element = imageRef.current;
    if (element) {
      element.style.transform = imageTransform(pan, pane.displayScale);
      element.style.willChange = panning ? 'transform' : '';
    }
    overlayHandle.current.reposition?.();
  }, [pane.displayScale]);

  // Runs after React writes the declarative transform, so the live gesture value
  // wins without the image snapping back if something else re-renders mid-drag.
  useLayoutEffect(() => {
    if (!dragOrigin.current) livePan.current = pane.pan;
    writeTransform(livePan.current, dragOrigin.current !== null);
  }, [pane.pan.x, pane.pan.y, pane.displayScale, imageUrl, writeTransform]);

  useEffect(() => () => {
    if (panFrame.current) cancelAnimationFrame(panFrame.current);
  }, []);

  const flushPan = useCallback(() => {
    panFrame.current = 0;
    writeTransform(livePan.current, true);
  }, [writeTransform]);

  const endPan = useCallback(() => {
    if (!dragOrigin.current) return;
    dragOrigin.current = null;
    if (panFrame.current) {
      cancelAnimationFrame(panFrame.current);
      panFrame.current = 0;
    }
    writeTransform(livePan.current, false);
    controller.updatePan(viewer, livePan.current);
  }, [controller, viewer, writeTransform]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    const updateViewport = () => {
      // Integer client sizes: fractional observer values drift across integer
      // boundaries during layout and fire repeated preview restarts.
      controller.setViewport(viewer, { width: stage.clientWidth, height: stage.clientHeight });
    };
    updateViewport();

    if (typeof ResizeObserver === 'undefined') return;
    // Defer to the next frame so writing layout from the callback cannot feed
    // back into the observer ("ResizeObserver loop completed" warning).
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(updateViewport);
    });
    observer.observe(stage);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [controller, viewer]);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    // React delegates `wheel` with a passive listener, so `preventDefault` in an
    // `onWheel` handler is ignored and logs "Unable to preventDefault inside
    // passive event listener invocation" on every scroll. Zooming keeps the
    // webview's own scroll/pinch gesture out of the way, which needs a native
    // non-passive listener.
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      controller.adjustZoom(viewer, event.deltaY < 0 ? 1 : -1);
    };
    stage.addEventListener('wheel', onWheel, { passive: false });
    return () => stage.removeEventListener('wheel', onWheel);
  }, [controller, viewer]);

  return (
    <article aria-label={`Viewer ${viewer}`} className={`viewer-pane${surface ? ' viewer-pane--surface' : ''}${className ? ` ${className}` : ''}`} style={style}>
      {!surface && (
        <div className="viewer-pane__header">
          <div>
            <span className="eyebrow">Viewer {viewer}</span>
            <strong>{pane.target ? `${pane.target.nodeName} · ${pane.target.outputName}` : 'No target selected'}</strong>
          </div>
          <TargetSelect controller={controller} options={options} pane={pane} viewer={viewer} />
        </div>
      )}
      <div
        className="viewer-pane__stage"
        ref={stageRef}
        onPointerDown={(event) => {
          // Record the origin before grabbing capture: a rejected pointer id must
          // not leave the gesture half-started.
          dragOrigin.current = {
            x: event.clientX,
            y: event.clientY,
            panX: livePan.current.x,
            panY: livePan.current.y,
          };
          try {
            event.currentTarget.setPointerCapture(event.pointerId);
          } catch {
            // Capture only keeps the drag alive outside the stage; the pan still
            // tracks the pointer while it stays over the image.
          }
        }}
        onPointerMove={(event) => {
          const origin = dragOrigin.current;
          if (!origin) return;
          livePan.current = {
            x: origin.panX + event.clientX - origin.x,
            y: origin.panY + event.clientY - origin.y,
          };
          // Coalesce to one DOM write per frame; pointer events can outpace paint.
          if (panFrame.current) return;
          panFrame.current = requestAnimationFrame(flushPan);
        }}
        onPointerUp={(event) => {
          endPan();
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onPointerCancel={(event) => {
          endPan();
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
        }}
      >
        {imageUrl ? (
          <img
            alt={pane.target ? `${pane.target.nodeName} preview` : 'Preview'}
            className="viewer-pane__image"
            crossOrigin="anonymous"
            // An <img> is natively draggable: the first pointer move hands the
            // gesture to the browser's image drag, which fires pointercancel and
            // freezes the pan. Opting out keeps the pointer ours for the whole
            // hold-and-drag gesture.
            draggable={false}
            height={pane.height ?? undefined}
            onDragStart={(event) => event.preventDefault()}
            onError={() => controller.reportImageLoadFailure(viewer, imageUrl)}
            onLoad={(event) => onAnalysis?.(analyzeImageElement(event.currentTarget))}
            ref={imageRef}
            src={imageUrl}
            style={{ transform: imageTransform(pane.pan, pane.displayScale) }}
            width={pane.width ?? undefined}
          />
        ) : pane.status === 'idle' || pane.status === 'cancelled' ? (
          <div className="viewer-pane__empty">
            {pane.status === 'cancelled' ? 'Preview cancelled' : 'Select an intermediate node output'}
          </div>
        ) : null}
        <ClippingOverlay
          analysis={analysis}
          enabled={clippingOverlay}
          handle={overlayHandle}
          imageRef={imageRef}
          stageRef={stageRef}
          viewer={viewer}
        />
        {pane.status === 'loading' && (
          <div className="viewer-pane__progress" role="status">
            <span>Rendering {Math.round(pane.progress * 100)}%</span>
            <progress max="1" value={pane.progress} />
          </div>
        )}
        {errorNotice && (
          <div className="viewer-pane__error" role="alert">
            <strong>{errorNotice.title}</strong>
            <span>{errorNotice.message}</span>
            <small>{errorNotice.guidance}</small>
            {pane.target && (
              <button
                aria-label={errorNotice.retryLabel}
                onClick={() => controller.setTarget(viewer, pane.target)}
                type="button"
              >
                Retry render
              </button>
            )}
          </div>
        )}
        {pane.target?.dataType === 'core.Mask' && paintedNode && (
          <MaskPainter
            imageOrigin={pane.imageOrigin}
            imageRef={imageRef}
            imageRegion={pane.imageRegion}
            node={paintedNode}
            onChange={(parameterId, value) => onPaintedMaskChange?.(paintedNode.id, parameterId, value)}
          />
        )}
      </div>
      {!surface && <div className="viewer-pane__controls">
        <button
          aria-pressed={pane.zoomMode === 'fit'}
          className={pane.zoomMode === 'fit' ? 'is-active' : ''}
          onClick={() => controller.fitToWindow(viewer)}
          type="button"
        >Fit</button>
        <button
          aria-pressed={pane.zoomMode === '100%'}
          className={pane.zoomMode === '100%' ? 'is-active' : ''}
          onClick={() => controller.viewAt100(viewer)}
          type="button"
        >100%</button>
        <button aria-label={`Zoom out Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, -1)} type="button"><Icon name="minus" /></button>
        <span className="viewer-pane__zoom">{Math.round(pane.zoom * 100)}%</span>
        <button aria-label={`Zoom in Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, 1)} type="button"><Icon name="plus" /></button>
        {pane.target?.dataType === 'core.Mask' && (
          <div className="viewer-pane__mask-display" role="group" aria-label={`Viewer ${viewer} mask display`}>
            <button
              aria-pressed={pane.maskDisplay === 'grayscale'}
              className={pane.maskDisplay === 'grayscale' ? 'is-active' : ''}
              onClick={() => controller.setMaskDisplay(viewer, 'grayscale')}
              type="button"
            >
              Mask
            </button>
            <button
              aria-pressed={pane.maskDisplay === 'overlay'}
              className={pane.maskDisplay === 'overlay' ? 'is-active' : ''}
              onClick={() => controller.setMaskDisplay(viewer, 'overlay')}
              type="button"
            >
              Overlay
            </button>
          </div>
        )}
        {pane.status === 'loading' && <button onClick={() => void controller.cancel(viewer)} type="button">Cancel</button>}
      </div>}
    </article>
  );
}

function ComparisonSurface({
  comparison,
  controller,
  options,
  analyses,
  clippingOverlay,
  onWipePositionChange,
  wipePosition,
  blinkViewer,
  paintedNode,
  onPaintedMaskChange,
  onAnalysis,
}: {
  comparison: Exclude<ViewerComparison, 'side-by-side'>;
  controller: ViewerController;
  options: PreviewTarget[];
  analyses: Record<ViewerId, ImageAnalysis | null>;
  clippingOverlay: boolean;
  onWipePositionChange: (position: number) => void;
  wipePosition: number;
  blinkViewer: ViewerId;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
  onAnalysis?: (viewer: ViewerId, analysis: ImageAnalysis | null) => void;
}) {
  const paneA = controller.state.panes.A;
  const paneB = controller.state.panes.B;
  const visiblePane = comparison === 'blink' ? blinkViewer : null;
  return (
    <div aria-label={`${comparison} comparison`} className={`viewer-comparison viewer-comparison--${comparison}`}>
      <div className="viewer-comparison__selectors">
        <label><span>Viewer A</span><TargetSelect controller={controller} options={options} pane={paneA} viewer="A" /></label>
        <label><span>Viewer B</span><TargetSelect controller={controller} options={options} pane={paneB} viewer="B" /></label>
      </div>
      <Pane
        analysis={analyses.A}
        className="viewer-pane--comparison-a"
        clippingOverlay={clippingOverlay}
        controller={controller}
        onAnalysis={(analysis) => onAnalysis?.('A', analysis)}
        onPaintedMaskChange={onPaintedMaskChange}
        options={options}
        paintedNode={paintedNode}
        pane={paneA}
        surface
        style={visiblePane === 'B' ? { visibility: 'hidden' } : undefined}
        viewer="A"
      />
      <Pane
        analysis={analyses.B}
        className="viewer-pane--comparison-b"
        clippingOverlay={clippingOverlay}
        controller={controller}
        onAnalysis={(analysis) => onAnalysis?.('B', analysis)}
        onPaintedMaskChange={onPaintedMaskChange}
        options={options}
        paintedNode={paintedNode}
        pane={paneB}
        surface
        style={{
          clipPath: comparison === 'wipe' ? `inset(0 ${100 - wipePosition}% 0 0)` : undefined,
          visibility: visiblePane === 'A' ? 'hidden' : undefined,
        }}
        viewer="B"
      />
      {comparison === 'wipe' && (
        <label className="viewer-comparison__wipe">
          <span className="sr-only">Wipe position</span>
          <input
            aria-label="Wipe position"
            max="100"
            min="0"
            onChange={(event) => onWipePositionChange(Number(event.target.value))}
            type="range"
            value={wipePosition}
          />
        </label>
      )}
      {comparison === 'blink' && <span aria-live="polite" className="viewer-comparison__status">Showing Viewer {blinkViewer}</span>}
    </div>
  );
}

export function Viewer({ controller, nodes, revision, source, paintedNode, onPaintedMaskChange, docked = false, collapsed = false, onToggleCollapsed }: ViewerProps) {
  const [, setRender] = useState(0);
  const options = useMemo(() => targetsFor(nodes), [nodes]);
  const [wipePosition, setWipePosition] = useState(50);
  const [showCompare, setShowCompare] = useState(false);
  const [blinkViewer, setBlinkViewer] = useState<ViewerId>('A');
  const [analyses, setAnalyses] = useState<Record<ViewerId, ImageAnalysis | null>>({ A: null, B: null });
  const [scopesVisible, setScopesVisible] = useState(true);
  const handleAnalysis = useCallback((viewer: ViewerId, analysis: ImageAnalysis | null) => {
    setAnalyses((current) => current[viewer] === analysis ? current : { ...current, [viewer]: analysis });
  }, []);

  const paneAnalysis = analyses.A ?? analyses.B;

  useEffect(() => controller.subscribe(() => setRender((value) => value + 1)), [controller]);
  useEffect(() => controller.setRevision(revision), [controller, revision]);
  useEffect(() => {
    setAnalyses((current) => {
      const next = {
        A: controller.state.panes.A.imageUrl ? current.A : null,
        B: controller.state.panes.B.imageUrl ? current.B : null,
      };
      return next.A === current.A && next.B === current.B ? current : next;
    });
  }, [controller, controller.state.panes.A.imageUrl, controller.state.panes.B.imageUrl]);
  useEffect(() => {
    if (controller.state.comparison !== 'blink') {
      setBlinkViewer('A');
      return;
    }
    const timer = window.setInterval(() => setBlinkViewer((viewer) => (viewer === 'A' ? 'B' : 'A')), 450);
    return () => window.clearInterval(timer);
  }, [controller.state.comparison]);
  useEffect(() => {
    if (controller.state.panes.B.target) setShowCompare(true);
  }, [controller.state.panes.B.target]);
  useEffect(() => {
    if (!source) {
      if (controller.state.panes.A.target || controller.state.panes.B.target) controller.clearTargets();
      return;
    }
    const available = (target: PreviewTarget | null) => target && options.some(
      (option) => option.nodeId === target.nodeId && option.outputPort === target.outputPort,
    );
    if (controller.state.panes.B.target && !available(controller.state.panes.B.target)) {
      controller.setTarget('B', null);
    }
    if (available(controller.state.panes.A.target)) return;
    const preferredNode = source.kind === 'raw'
      ? nodes.find((node) => node.typeId === 'raw.display-transform')
      : nodes.find((node) => node.typeId === 'core.output');
    const target = options.find((option) => option.nodeId === preferredNode?.id)
      ?? options.find((option) => option.dataType === 'core.Image' || option.dataType === 'color.DisplayRGB')
      ?? options[0];
    controller.setTarget('A', target ?? null);
  }, [controller, nodes, options, source?.kind, source?.revision]);

  return (
    <section
      aria-label="Image viewers"
      className={`viewer-section${docked ? ' viewer-section--docked' : ''}${collapsed ? ' viewer-section--collapsed' : ''}${!collapsed && scopesVisible && paneAnalysis ? ' viewer-section--scopes' : ''}`}
    >
      <div className="viewer-section__toolbar">
        <div aria-label="Viewer comparison" className="viewer-section__comparison" role="group">
          {COMPARISON_BUTTONS.map(({ id, label, short }) => {
            const active = showCompare && controller.state.comparison === id;
            return (
              <button
                aria-label={label}
                aria-pressed={active}
                className={active ? 'is-active' : ''}
                key={id}
                onClick={() => {
                  // "Compare A and B" selects the side-by-side comparison. It only
                  // toggles that surface off when it is already the one being shown:
                  // arriving from wipe/blink/difference must reveal the two viewers
                  // instead of hiding them, which left the button's label, its
                  // pressed state, and the visible surface disagreeing.
                  const alreadyShowing = showCompare && controller.state.comparison === id;
                  controller.setComparison(id);
                  setShowCompare(id === 'side-by-side' ? !alreadyShowing : true);
                }}
                title={label}
                type="button"
              >
                {short}
              </button>
            );
          })}
          {showCompare && controller.state.comparison === 'side-by-side' && (
            <>
              <button
                aria-label="Viewer layout: side by side"
                aria-pressed={controller.state.layout === 'side-by-side'}
                className={controller.state.layout === 'side-by-side' ? 'is-active' : ''}
                onClick={() => controller.setLayout('side-by-side')}
                title="Stack the two viewers horizontally"
                type="button"
              >
                ⇋
              </button>
              <button
                aria-label="Viewer layout: stacked"
                aria-pressed={controller.state.layout === 'split'}
                className={controller.state.layout === 'split' ? 'is-active' : ''}
                onClick={() => controller.setLayout('split')}
                title="Stack the two viewers vertically"
                type="button"
              >
                ⇅
              </button>
            </>
          )}
          <button
            aria-label={scopesVisible ? 'Hide scopes' : 'Show scopes'}
            aria-pressed={scopesVisible}
            className={`viewer-section__scopes-toggle${scopesVisible ? ' is-active' : ''}`}
            onClick={() => setScopesVisible((visible) => !visible)}
            title={scopesVisible ? 'Hide scopes' : 'Show scopes'}
            type="button"
          >
            Scopes
          </button>
          <label className="viewer-section__clipping" title="Highlight clipped highlights and shadows">
            <input
              aria-label="Clipping"
              checked={controller.state.clippingOverlay}
              onChange={(event) => controller.setClippingOverlay(event.target.checked)}
              type="checkbox"
            />
            Clip
          </label>
        </div>
        {onToggleCollapsed && (
          <button
            aria-label={collapsed ? 'Expand preview panel' : 'Collapse preview panel'}
            className="icon-button"
            onClick={onToggleCollapsed}
            title={collapsed ? 'Expand preview panel' : 'Collapse preview panel'}
            type="button"
          >
            {collapsed ? <Icon name="chevronRight" /> : <Icon name="chevronDown" />}
          </button>
        )}
      </div>
      {!collapsed && (controller.state.comparison === 'side-by-side' ? (
        <div className={`viewer-grid viewer-grid--${showCompare ? controller.state.layout : 'single'}`}>
          <Pane
            analysis={analyses.A}
            clippingOverlay={controller.state.clippingOverlay}
            controller={controller}
            onAnalysis={(analysis) => handleAnalysis('A', analysis)}
            onPaintedMaskChange={onPaintedMaskChange}
            options={options}
            paintedNode={paintedNode}
            pane={controller.state.panes.A}
            viewer="A" />
          {showCompare && <Pane
            analysis={analyses.B}
            clippingOverlay={controller.state.clippingOverlay}
            controller={controller}
            onAnalysis={(analysis) => handleAnalysis('B', analysis)}
            onPaintedMaskChange={onPaintedMaskChange}
            options={options}
            paintedNode={paintedNode}
            pane={controller.state.panes.B}
            viewer="B" />}
        </div>
      ) : (
        <ComparisonSurface
          analyses={analyses}
          blinkViewer={blinkViewer}
          clippingOverlay={controller.state.clippingOverlay}
          comparison={controller.state.comparison}
          controller={controller}
          onAnalysis={handleAnalysis}
          onPaintedMaskChange={onPaintedMaskChange}
          onWipePositionChange={setWipePosition}
          options={options}
          paintedNode={paintedNode}
          wipePosition={wipePosition}
        />
      ))}
      {!collapsed && scopesVisible && paneAnalysis && <Scopes analysis={paneAnalysis} compact={docked} />}
    </section>
  );
}
