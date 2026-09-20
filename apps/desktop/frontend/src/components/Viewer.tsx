import { useEffect, useMemo, useRef, useState } from 'react';
import type { EditorNode, OpenImageResult, ParameterValue } from '../editor/types';
import { MaskPainter } from '../mask/MaskPainter';
import { ViewerController } from '../viewer/controller';
import type { PreviewTarget, ViewerId, ViewerPaneState } from '../viewer/types';

interface ViewerProps {
  controller: ViewerController;
  nodes: EditorNode[];
  revision: number;
  source: OpenImageResult | null;
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

function Pane({ viewer, pane, options, controller, paintedNode, onPaintedMaskChange }: {
  viewer: ViewerId;
  pane: ViewerPaneState;
  options: PreviewTarget[];
  controller: ViewerController;
  paintedNode?: EditorNode;
  onPaintedMaskChange?: (nodeId: string, parameterId: string, value: ParameterValue) => void;
}) {
  const dragStart = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const selectedKey = pane.target ? targetKey(pane.target) : '';

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
    <article className="viewer-pane" aria-label={`Viewer ${viewer}`}>
      <div className="viewer-pane__header">
        <div>
          <span className="eyebrow">Viewer {viewer}</span>
          <strong>{pane.target ? `${pane.target.nodeName} · ${pane.target.outputName}` : 'No target selected'}</strong>
        </div>
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
      </div>
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
      <div className="viewer-pane__controls">
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
      </div>
    </article>
  );
}

export function Viewer({ controller, nodes, revision, source, paintedNode, onPaintedMaskChange }: ViewerProps) {
  const [, setRender] = useState(0);
  const options = useMemo(() => targetsFor(nodes), [nodes]);

  useEffect(() => controller.subscribe(() => setRender((value) => value + 1)), [controller]);
  useEffect(() => controller.setRevision(revision), [controller, revision]);
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
      </div>
      <div className={`viewer-grid viewer-grid--${controller.state.layout}`}>
        <Pane
          controller={controller}
          onPaintedMaskChange={onPaintedMaskChange}
          options={options}
          paintedNode={paintedNode}
          pane={controller.state.panes.A}
          viewer="A"
        />
        <Pane
          controller={controller}
          onPaintedMaskChange={onPaintedMaskChange}
          options={options}
          paintedNode={paintedNode}
          pane={controller.state.panes.B}
          viewer="B"
        />
      </div>
    </section>
  );
}
