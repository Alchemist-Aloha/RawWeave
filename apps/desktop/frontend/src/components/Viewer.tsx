import { useEffect, useMemo, useRef, useState } from 'react';
import type { EditorNode } from '../editor/types';
import { ViewerController } from '../viewer/controller';
import type { PreviewTarget, ViewerId, ViewerPaneState } from '../viewer/types';

interface ViewerProps {
  controller: ViewerController;
  nodes: EditorNode[];
  revision: number;
}

function targetsFor(nodes: EditorNode[]): PreviewTarget[] {
  return nodes.flatMap((node) =>
    node.descriptor.outputs
      .filter((output) => output.dataType === 'core.Image')
      .map((output) => ({
        nodeId: node.id,
        nodeName: node.descriptor.name,
        outputPort: output.id,
        outputName: output.name,
      })),
  );
}

function targetKey(target: PreviewTarget): string {
  return `${target.nodeId}:${target.outputPort}`;
}

function Pane({ viewer, pane, options, controller }: {
  viewer: ViewerId;
  pane: ViewerPaneState;
  options: PreviewTarget[];
  controller: ViewerController;
}) {
  const dragStart = useRef<{ x: number; y: number; panX: number; panY: number } | null>(null);
  const selectedKey = pane.target ? targetKey(pane.target) : '';

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
            src={pane.imageUrl}
            style={{
              transform: `translate(${pane.pan.x}px, ${pane.pan.y}px) scale(${pane.zoom})`,
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
      </div>
      <div className="viewer-pane__controls">
        <button onClick={() => controller.fitToWindow(viewer)} type="button">Fit</button>
        <button onClick={() => controller.viewAt100(viewer)} type="button">100%</button>
        <button aria-label={`Zoom out Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, -1)} type="button">−</button>
        <span className="viewer-pane__zoom">{pane.zoomMode === 'fit' ? 'Fit' : `${Math.round(pane.zoom * 100)}%`}</span>
        <button aria-label={`Zoom in Viewer ${viewer}`} onClick={() => controller.adjustZoom(viewer, 1)} type="button">+</button>
        {pane.status === 'loading' && <button onClick={() => void controller.cancel(viewer)} type="button">Cancel</button>}
      </div>
    </article>
  );
}

export function Viewer({ controller, nodes, revision }: ViewerProps) {
  const [, setRender] = useState(0);
  const options = useMemo(() => targetsFor(nodes), [nodes]);

  useEffect(() => controller.subscribe(() => setRender((value) => value + 1)), [controller]);
  useEffect(() => controller.setRevision(revision), [controller, revision]);

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
        <Pane controller={controller} options={options} pane={controller.state.panes.A} viewer="A" />
        <Pane controller={controller} options={options} pane={controller.state.panes.B} viewer="B" />
      </div>
    </section>
  );
}
