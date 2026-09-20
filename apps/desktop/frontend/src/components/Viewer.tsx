import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import type { EditorNode, ParameterValue, SourceResult } from '../editor/types';
import { MaskPainter } from '../mask/MaskPainter';
import { ViewerController } from '../viewer/controller';
import type { PreviewTarget, ViewerComparison, ViewerId, ViewerPaneState } from '../viewer/types';

interface ViewerProps {
  controller: ViewerController;
  nodes: EditorNode[];
  revision: number;
  source: SourceResult | null;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
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

function Pane({ viewer, pane, options, controller, paintedNode, onPaintedMaskChange, surface = false, clippingOverlay = false, className = '', style }: {
  viewer: ViewerId;
  pane: ViewerPaneState;
  options: PreviewTarget[];
  controller: ViewerController;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
  surface?: boolean;
  clippingOverlay?: boolean;
  className?: string;
  style?: CSSProperties;
}) {
  const dragStart = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    const updateViewport = (width: number, height: number) => {
      controller.setViewport(viewer, { width, height });
    };
    const measure = () => updateViewport(stage.clientWidth, stage.clientHeight);
    measure();

    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(([entry]) => {
      const width = entry?.contentRect.width ?? stage.clientWidth;
      const height = entry?.contentRect.height ?? stage.clientHeight;
      updateViewport(width, height);
    });
    observer.observe(stage);
    return () => observer.disconnect();
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
          event.currentTarget.setPointerCapture(event.pointerId);
          dragStart.current = {
            x: event.clientX,
            y: event.clientY,
            panX: pane.pan.x,
            panY: pane.pan.y,
          };
        }}
        onPointerMove={(event) => {
          if (!dragStart.current) return;
          controller.setPan(viewer, {
            x: dragStart.current.panX + event.clientX - dragStart.current.x,
            y: dragStart.current.panY + event.clientY - dragStart.current.y,
          });
        }}
        onPointerUp={() => {
          dragStart.current = null;
        }}
        onPointerCancel={() => {
          dragStart.current = null;
        }}
        onWheel={(event) => {
          event.preventDefault();
          controller.adjustZoom(viewer, event.deltaY < 0 ? 1 : -1);
        }}
      >
        {pane.imageUrl ? (
          <img
            alt={pane.target ? `${pane.target.nodeName} preview` : 'Preview'}
            className={`viewer-pane__image${pane.zoomMode === 'fit' ? ' viewer-pane__image--fit' : ''}`}
            height={pane.height ?? undefined}
            ref={imageRef}
            src={pane.imageUrl}
            style={{
              transform: `translate(${pane.pan.x}px, ${pane.pan.y}px) scale(${pane.displayScale})`,
            }}
            width={pane.width ?? undefined}
          />
        ) : (
          <div className="viewer-pane__empty">
            {pane.status === 'loading' ? 'Rendering preview…' : 'Select an intermediate node output'}
          </div>
        )}
        {clippingOverlay && <div aria-label={`Viewer ${viewer} clipping overlay`} className="viewer-pane__clipping-overlay" role="img" />}
        {pane.status === 'loading' && (
          <div className="viewer-pane__progress" role="status">
            <span>Rendering {Math.round(pane.progress * 100)}%</span>
            <progress max="1" value={pane.progress} />
          </div>
        )}
        {pane.status === 'error' && <div className="viewer-pane__error">{pane.error}</div>}
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
        <button onClick={() => controller.fitToWindow(viewer)} type="button">Fit</button>
        <button onClick={() => controller.viewAt100(viewer)} type="button">100%</button>
        <button aria-label={`Zoom out Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, -1)} type="button">−</button>
        <span className="viewer-pane__zoom">{pane.zoomMode === 'fit' ? 'Fit' : `${Math.round(pane.zoom * 100)}%`}</span>
        <button aria-label={`Zoom in Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, 1)} type="button">+</button>
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
  clippingOverlay,
  onWipePositionChange,
  wipePosition,
  blinkViewer,
  paintedNode,
  onPaintedMaskChange,
}: {
  comparison: Exclude<ViewerComparison, 'side-by-side'>;
  controller: ViewerController;
  options: PreviewTarget[];
  clippingOverlay: boolean;
  onWipePositionChange: (position: number) => void;
  wipePosition: number;
  blinkViewer: ViewerId;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
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
        className="viewer-pane--comparison-a"
        clippingOverlay={clippingOverlay}
        controller={controller}
        onPaintedMaskChange={onPaintedMaskChange}
        options={options}
        paintedNode={paintedNode}
        pane={paneA}
        surface
        style={visiblePane === 'B' ? { visibility: 'hidden' } : undefined}
        viewer="A"
      />
      <Pane
        className="viewer-pane--comparison-b"
        clippingOverlay={clippingOverlay}
        controller={controller}
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

export function Viewer({ controller, nodes, revision, source, paintedNode, onPaintedMaskChange }: ViewerProps) {
  const [, setRender] = useState(0);
  const options = useMemo(() => targetsFor(nodes), [nodes]);
  const [wipePosition, setWipePosition] = useState(50);
  const [blinkViewer, setBlinkViewer] = useState<ViewerId>('A');

  useEffect(() => controller.subscribe(() => setRender((value) => value + 1)), [controller]);
  useEffect(() => controller.setRevision(revision), [controller, revision]);
  useEffect(() => {
    if (controller.state.comparison !== 'blink') {
      setBlinkViewer('A');
      return;
    }
    const timer = window.setInterval(() => setBlinkViewer((viewer) => (viewer === 'A' ? 'B' : 'A')), 450);
    return () => window.clearInterval(timer);
  }, [controller.state.comparison]);
  useEffect(() => {
    if (source?.kind !== 'raw') {
      controller.clearTargets();
      return;
    }
    const displayTarget = options.find(
      (option) => option.nodeName === 'Display Transform' && option.outputPort === 'display',
    );
    if (!displayTarget || controller.state.panes.A.target?.nodeId === displayTarget.nodeId) return;
    controller.setTarget('A', displayTarget);
  }, [controller, options, source?.kind, source?.revision]);

  return (
    <section className="viewer-section" aria-label="Image viewers">
      <div className="viewer-section__toolbar">
        <div>
          <span className="eyebrow">Viewer</span>
          <strong>Intermediate image preview</strong>
        </div>
        <div className="viewer-section__layout" role="group" aria-label="Viewer layout">
          <button className={controller.state.layout === 'side-by-side' ? 'is-active' : ''} onClick={() => controller.setLayout('side-by-side')} type="button">
            Side by side
          </button>
          <button className={controller.state.layout === 'split' ? 'is-active' : ''} onClick={() => controller.setLayout('split')} type="button">
            Split
          </button>
        </div>
        <div aria-label="Viewer comparison" className="viewer-section__comparison" role="group">
          {(['side-by-side', 'wipe', 'blink', 'difference'] as ViewerComparison[]).map((comparison) => (
            <button
              aria-pressed={controller.state.comparison === comparison}
              className={controller.state.comparison === comparison ? 'is-active' : ''}
              key={comparison}
              onClick={() => controller.setComparison(comparison)}
              type="button"
            >
              {comparison === 'side-by-side' ? 'A / B' : comparison[0].toUpperCase() + comparison.slice(1)}
            </button>
          ))}
          <label className="viewer-section__clipping">
            <input
              checked={controller.state.clippingOverlay}
              onChange={(event) => controller.setClippingOverlay(event.target.checked)}
              type="checkbox"
            />
            Clipping
          </label>
        </div>
      </div>
      {controller.state.comparison === 'side-by-side' ? (
        <div className={`viewer-grid viewer-grid--${controller.state.layout}`}>
          <Pane
            clippingOverlay={controller.state.clippingOverlay}
            controller={controller}
            onPaintedMaskChange={onPaintedMaskChange}
            options={options}
            paintedNode={paintedNode}
            pane={controller.state.panes.A}
            viewer="A"
          />
          <Pane
            clippingOverlay={controller.state.clippingOverlay}
            controller={controller}
            onPaintedMaskChange={onPaintedMaskChange}
            options={options}
            paintedNode={paintedNode}
            pane={controller.state.panes.B}
            viewer="B"
          />
        </div>
      ) : (
        <ComparisonSurface
          blinkViewer={blinkViewer}
          clippingOverlay={controller.state.clippingOverlay}
          comparison={controller.state.comparison}
          controller={controller}
          onPaintedMaskChange={onPaintedMaskChange}
          onWipePositionChange={setWipePosition}
          options={options}
          paintedNode={paintedNode}
          wipePosition={wipePosition}
        />
      )}
    </section>
  );
}
