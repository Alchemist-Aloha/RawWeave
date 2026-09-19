import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  type Connection,
  type Edge,
  type Node,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { EditorController } from './editor/controller';
import type { OpenImageResult, ParameterValue, WorkflowMetadata } from './editor/types';
import { createPlatform } from './platform/editor';
import { GraphNode, type RawWeaveFlowNode } from './components/GraphNode';
import { Inspector } from './components/Inspector';
import { NodeLibrary } from './components/NodeLibrary';
import { targetsFor, Viewer } from './components/Viewer';
import { ViewerController } from './viewer/controller';
import { createPreviewTransport } from './platform/preview';
import { BrowserQueue } from './browser/BrowserQueue';
import type { BrowserSession } from './browser/types';
import type { BatchWorkflowContext } from './batch/model';
import { createBatchPlatform } from './platform/batch';
import { HostManager } from './components/HostManager';
import { createTauriHostManager } from './platform/hosts';

const nodeTypes = { rawweave: GraphNode };

function downloadWorkflow(contents: string, filename = 'rawweave-workflow.json'): void {
  const blob = new Blob([contents], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

const RAW_EXTENSIONS = [
  '.3fr',
  '.arw',
  '.cr2',
  '.cr3',
  '.dcr',
  '.dng',
  '.erf',
  '.kdc',
  '.mrw',
  '.nef',
  '.nrw',
  '.orf',
  '.pef',
  '.raf',
  '.raw',
  '.rw2',
  '.rwl',
  '.srw',
  '.x3f',
];
const IMAGE_ACCEPT = ['image/*', ...RAW_EXTENSIONS].join(',');

function unavailable(value: unknown): string {
  return value === null || value === undefined || value === '' ? 'Unavailable' : String(value);
}

function SourceMetadata({ source }: { source: OpenImageResult | null }) {
  if (!source) return null;
  const metadata = source.metadata;
  const rows = metadata
    ? [
        ['Camera', metadata.camera],
        ['Lens', metadata.lens],
        ['ISO', metadata.iso],
        ['Aperture', metadata.aperture === null ? null : `f/${metadata.aperture}`],
        ['Shutter', metadata.shutter === null ? null : `${metadata.shutter}s`],
        ['Focal length', metadata.focalLength === null ? null : `${metadata.focalLength}mm`],
        ['Capture time', metadata.captureTime],
        ['Orientation', metadata.orientation],
        ['Dimensions', `${metadata.dimensions.width} × ${metadata.dimensions.height}`],
        ['EXIF tags', Object.keys(metadata.exif).length || null],
      ]
    : [
        ['Camera', null],
        ['Lens', null],
        ['ISO', null],
        ['Aperture', null],
        ['Shutter', null],
        ['Focal length', null],
        ['Capture time', null],
        ['Orientation', null],
        ['Dimensions', `${source.width} × ${source.height}`],
        ['EXIF tags', null],
      ];
  return (
    <section aria-label="Source metadata" className="source-metadata">
      <div>
        <span className="eyebrow">Source</span>
        <strong>{source.kind === 'raw' ? 'RAW image' : 'Image'}</strong>
      </div>
      <dl>
        {rows.map(([label, value]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{unavailable(value)}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function SubgraphForm({
  onCancel,
  onCreate,
}: {
  onCancel: () => void;
  onCreate: (options: { id: string; version: string; metadata: WorkflowMetadata }) => void;
}) {
  const [id, setId] = useState('my-subgraph');
  const [version, setVersion] = useState('1.0.0');
  const [name, setName] = useState('My Subgraph');
  const [description, setDescription] = useState('');
  return (
    <form className="subgraph-form" onSubmit={(event) => {
      event.preventDefault();
      onCreate({ id: id.trim(), version: version.trim(), metadata: { name: name.trim(), description } });
    }}>
      <div className="subgraph-form__heading">
        <div>
          <span className="eyebrow">Reusable component</span>
          <strong>Create subgraph from selection</strong>
        </div>
        <button className="icon-button" onClick={onCancel} type="button">×</button>
      </div>
      <div className="subgraph-form__fields">
        <label>Identity<input required value={id} onChange={(event) => setId(event.target.value)} /></label>
        <label>Version<input required value={version} onChange={(event) => setVersion(event.target.value)} /></label>
        <label>Name<input required value={name} onChange={(event) => setName(event.target.value)} /></label>
        <label>Description<input value={description} onChange={(event) => setDescription(event.target.value)} /></label>
      </div>
      <div className="subgraph-form__actions">
        <button className="button button--quiet" onClick={onCancel} type="button">Cancel</button>
        <button className="button button--primary" type="submit">Create and open</button>
      </div>
    </form>
  );
}

function DependencySummary({
  report,
  hash,
}: {
  report: EditorController['state']['dependencyReport'];
  hash: string | null;
}) {
  if (!report) return <span className="canvas-panel__meta">Checking dependencies…</span>;
  const problemCount = report.missing.length + report.mismatched.length + report.disabledNodes.length;
  return (
    <div className={`workflow-health${problemCount ? ' workflow-health--warning' : ''}`} title={hash ?? 'Hash unavailable'}>
      <span className="workflow-health__dot" />
      <span>{problemCount ? `${problemCount} dependency issue${problemCount === 1 ? '' : 's'}` : 'Dependencies ready'}</span>
      {hash && <code>#{hash.slice(0, 12)}</code>}
    </div>
  );
}

export default function App() {
  const [platform] = useState(() => createPlatform());
  const [batchPlatform] = useState(() => createBatchPlatform());
  const [controller] = useState(() => new EditorController(platform));
  const [hostManager] = useState(() => createTauriHostManager());
  const [viewerController] = useState(() => new ViewerController(createPreviewTransport()));
  const [, setRevision] = useState(0);
  const [, setViewerRevision] = useState(0);
  const [restoredSession, setRestoredSession] = useState<BrowserSession | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [showSubgraphForm, setShowSubgraphForm] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const blueprintInput = useRef<HTMLInputElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  const refreshEditorDescriptors = useCallback(
    () => controller.refreshDescriptors(),
    [controller],
  );

  useEffect(() => {
    const unsubscribe = controller.subscribe(() => setRevision((revision) => revision + 1));
    void controller.initialize().catch(() => undefined);
    return unsubscribe;
  }, [controller]);

  useEffect(() => viewerController.subscribe(() => setViewerRevision((revision) => revision + 1)), [viewerController]);

  const flowNodes = useMemo<RawWeaveFlowNode[]>(
    () =>
      controller.state.nodes.map((node) => ({
        id: node.id,
        type: 'rawweave',
        position: node.position,
        data: { node },
        selected: controller.state.selectedNodeIds.includes(node.id),
      })),
    [controller, controller.state.nodes, controller.state.selectedNodeIds],
  );
  const flowEdges = useMemo<Edge[]>(
    () =>
      controller.state.edges.map((edge) => ({
        id: edge.id,
        source: edge.source,
        sourceHandle: edge.sourceHandle,
        target: edge.target,
        targetHandle: edge.targetHandle,
        animated: false,
        type: 'smoothstep',
      })),
    [controller, controller.state.edges],
  );

  const selectedNode = controller.state.nodes.find(
    (node) => node.id === controller.state.selectedNodeId,
  );
  const browserWorkflowBinding = useMemo(() => {
    const scope = controller.state.scopePath.at(-1);
    if (!scope || !controller.state.workflowHash) return null;
    return { id: scope.id, version: scope.version ?? '1.0.0', hash: controller.state.workflowHash };
  }, [controller.state.scopePath, controller.state.workflowHash]);

  const batchWorkflow = useMemo<BatchWorkflowContext | null>(() => {
    if (!browserWorkflowBinding) return null;
    const definition = controller.state.blueprint;
    const scope = controller.state.scopePath.at(-1);
    return {
      binding: browserWorkflowBinding,
      revision: controller.state.revision,
      nodes: controller.state.nodes,
      edges: controller.state.edges,
      parameters: controller.state.workflowParameters,
      inputs: controller.state.workflowInputs,
      outputs: controller.state.workflowOutputs,
      metadata: definition?.metadata ?? { name: scope?.name ?? 'Workflow' },
      nodePackDependencies: definition?.nodePackDependencies ?? [],
      subgraphDependencies: definition?.subgraphDependencies ?? [],
    };
  }, [browserWorkflowBinding, controller.state.blueprint, controller.state.edges, controller.state.nodes, controller.state.revision, controller.state.scopePath, controller.state.workflowInputs, controller.state.workflowOutputs, controller.state.workflowParameters]);

  const onConnect = useCallback(
    (connection: Connection) => {
      if (!connection.sourceHandle || !connection.targetHandle) return;
      void controller
        .connect(
          connection.source,
          connection.sourceHandle,
          connection.target,
          connection.targetHandle,
        )
        .catch(() => undefined);
    },
    [controller],
  );

  const onNodesChange = useCallback(
    (changes: any[]) => {
      for (const change of changes) {
        if (change.type === 'position' && change.position) {
          controller.updateNodePosition(change.id, change.position);
        } else if (change.type === 'remove') {
          void controller.removeNode(change.id).catch(() => undefined);
        }
      }
    },
    [controller],
  );

  const onEdgesChange = useCallback(
    (changes: any[]) => {
      for (const change of changes) {
        if (change.type !== 'remove') continue;
        const edge = controller.state.edges.find((candidate) => candidate.id === change.id);
        if (edge) {
          void controller
            .disconnect(edge.fromNode, edge.fromPort, edge.toNode, edge.toPort)
            .catch(() => undefined);
        }
      }
    },
    [controller],
  );

  const onParameterChange = useCallback(
    (parameterId: string, value: ParameterValue) => {
      if (!selectedNode) return;
      void controller.setParameter(selectedNode.id, parameterId, value).catch(() => undefined);
    },
    [controller, selectedNode],
  );

  const onPaintedMaskChange = useCallback(
    (nodeId: string, parameterId: string, value: ParameterValue) => {
      void controller.setParameter(nodeId, parameterId, value).catch(() => undefined);
    },
    [controller],
  );

  const onToggleExposed = useCallback(
    (parameterId: string, exposed: boolean) => {
      if (!selectedNode) return;
      const action = exposed
        ? controller.exposeParameter(selectedNode.id, parameterId)
        : controller.unexposeParameter(selectedNode.id, parameterId);
      void action.catch(() => undefined);
    },
    [controller, selectedNode],
  );

  const onToggleInput = useCallback(
    (nodeId: string, portId: string, direction: 'Input' | 'Output', exposed: boolean) => {
      const action = exposed
        ? direction === 'Input'
          ? controller.exposeInput(nodeId, portId)
          : controller.exposeOutput(nodeId, portId)
        : controller.hidePort(`${direction === 'Input' ? 'input' : 'output'}:${nodeId}:${portId}`);
      void action.catch(() => undefined);
    },
    [controller],
  );

  const promoteOverrides = useCallback(async (overrides: Record<string, ParameterValue>) => {
    for (const [workflowParameterId, value] of Object.entries(overrides)) {
      await controller.setWorkflowParameter(workflowParameterId, value);
    }
  }, [controller]);

  const onBrowserSessionLoaded = useCallback((session: BrowserSession) => {
    setRestoredSession(session);
    if (session.panelLayout === 'side-by-side' || session.panelLayout === 'split') {
      viewerController.setLayout(session.panelLayout);
    }
  }, [viewerController]);

  useEffect(() => {
    if (!restoredSession || !controller.state.source) return;
    const options = targetsFor(controller.state.nodes);
    for (const viewer of ['A', 'B'] as const) {
      const savedTarget = restoredSession.viewer.targets[viewer];
      if (!savedTarget) continue;
      const target = options.find((option) => option.nodeId === savedTarget.nodeId && option.outputPort === savedTarget.outputPort);
      if (target
        && (viewerController.state.panes[viewer].target?.nodeId !== target.nodeId
          || viewerController.state.panes[viewer].target?.outputPort !== target.outputPort)) {
        viewerController.setTarget(viewer, target);
      }
    }
  }, [controller.state.nodes, controller.state.source, restoredSession, viewerController]);

  const persistedViewerTargets = useMemo<BrowserSession['viewer']['targets']>(() => ({
    A: viewerController.state.panes.A.target
      ? { nodeId: viewerController.state.panes.A.target.nodeId, outputPort: viewerController.state.panes.A.target.outputPort }
      : null,
    B: viewerController.state.panes.B.target
      ? { nodeId: viewerController.state.panes.B.target.nodeId, outputPort: viewerController.state.panes.B.target.outputPort }
      : null,
  }), [viewerController.state.panes.A.target, viewerController.state.panes.B.target]);

  const createSubgraph = useCallback(
    (options: { id: string; version: string; metadata: WorkflowMetadata }) => {
      void controller
        .createSubgraphFromSelection(controller.state.selectedNodeIds, options)
        .then(() => setShowSubgraphForm(false))
        .catch(() => undefined);
    },
    [controller],
  );

  const loadBlueprint = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = '';
      if (!file) return;
      await controller.importBlueprint(await file.text()).catch(() => undefined);
    },
    [controller],
  );

  const navigateToScope = useCallback(
    async (index: number) => {
      while (controller.state.scopePath.length - 1 > index) {
        try {
          await controller.returnToParent();
        } catch {
          return;
        }
      }
    },
    [controller],
  );

  const openNestedSubgraph = useCallback(
    (id: string) => {
      void controller.openSubgraph(id).catch(() => undefined);
    },
    [controller],
  );

  const onSelectionChange = useCallback(
    ({ nodes }: { nodes: Node[] }) => controller.selectNodes(nodes.map((node) => node.id)),
    [controller],
  );

  const toggleNodeSelection = useCallback(
    (event: React.MouseEvent, nodeId: string) => {
      if (!event.metaKey && !event.ctrlKey && !event.shiftKey) {
        controller.selectNode(nodeId);
        return;
      }
      const selected = new Set(controller.state.selectedNodeIds);
      if (selected.has(nodeId)) selected.delete(nodeId);
      else selected.add(nodeId);
      controller.selectNodes([...selected]);
    },
    [controller],
  );

  const blueprintAction = useCallback(
    async (action: 'save' | 'export') => {
      const serialized = action === 'save' ? await controller.saveBlueprint() : await controller.exportBlueprint();
      downloadWorkflow(serialized, `rawweave-blueprint-${action}.json`);
    },
    [controller],
  );

  const instantiateBlueprint = useCallback(() => {
    void controller.instantiateBlueprint().catch(() => undefined);
  }, [controller]);

  const loadFile = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = '';
      if (!file) return;
      await controller.loadWorkflow(await file.text()).catch(() => undefined);
    },
    [controller],
  );

  const openImagePath = useCallback(
    async (path: string) => {
      try {
        const image = await controller.openImage(path);
        viewerController.setSourceDimensions(image);
        setImageError(null);
      } catch (error) {
        setImageError(error instanceof Error ? error.message : String(error));
      }
    },
    [controller, viewerController],
  );

  const chooseImage = useCallback(
    async (event: React.ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      event.target.value = '';
      if (!file) return;
      await openImagePath(file.name);
    },
    [openImagePath],
  );

  const openImage = useCallback(async () => {
    if (!isTauri) {
      imageInput.current?.click();
      return;
    }
    try {
      const path = await platform.chooseImagePath();
      if (path) await openImagePath(path);
    } catch (error) {
      setImageError(error instanceof Error ? error.message : String(error));
    }
  }, [isTauri, openImagePath, platform]);

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-mark">
          <span className="brand-mark__glyph">RW</span>
          <span>
            <strong>RawWeave</strong>
            <small>node graph editor</small>
          </span>
        </div>
        <div className="topbar__actions">
          <button className="button button--quiet" onClick={() => void openImage()} type="button">
            Open Image
          </button>
          <button className="button button--quiet" onClick={() => fileInput.current?.click()} type="button">
            Open Workflow
          </button>
          <button
            className="button button--primary"
            onClick={() => void controller.saveWorkflow().then(downloadWorkflow).catch(() => undefined)}
            type="button"
          >
            Save workflow
          </button>
          <button
            className="button button--quiet"
            onClick={() => blueprintInput.current?.click()}
            type="button"
          >
            Import blueprint
          </button>
          <button
            className="button button--quiet"
            onClick={() => void blueprintAction('save').catch(() => undefined)}
            type="button"
          >
            Save blueprint
          </button>
          <button
            className="button button--quiet"
            onClick={() => void blueprintAction('export').catch(() => undefined)}
            type="button"
          >
            Export blueprint
          </button>
          <button
            className="button button--quiet"
            disabled={!controller.state.blueprint}
            onClick={instantiateBlueprint}
            type="button"
          >
            Instantiate
          </button>
          <input
            accept="application/json,.json"
            className="sr-only"
            onChange={loadBlueprint}
            ref={blueprintInput}
            type="file"
          />
          <input
            accept={IMAGE_ACCEPT}
            className="sr-only"
            onChange={chooseImage}
            ref={imageInput}
            type="file"
          />
          <input
            accept="application/json,.json"
            className="sr-only"
            onChange={loadFile}
            ref={fileInput}
            type="file"
          />
        </div>
      </header>

      <HostManager api={hostManager} onDiscovery={refreshEditorDescriptors} />

      <SourceMetadata source={controller.state.source} />
      <BrowserQueue
        batchPlatform={batchPlatform}
        batchWorkflow={batchWorkflow}
        onOpenImage={openImagePath}
        onOpenFailedItem={(item) => openImagePath(item.sourcePath)}
        onPromoteOverrides={promoteOverrides}
        onSessionLoaded={onBrowserSessionLoaded}
        panelLayout={viewerController.state.layout}
        viewerTargets={persistedViewerTargets}
        workflowBinding={browserWorkflowBinding}
        workflowParameters={controller.state.workflowParameters}
      />

      <section className="workspace">
        <NodeLibrary
          descriptors={controller.state.descriptors}
          onAdd={(typeId) => void controller.createNode(typeId).catch(() => undefined)}
          onCreateSubgraph={() => setShowSubgraphForm(true)}
          selectedCount={controller.state.selectedNodeIds.length}
        />
        <section className="canvas-panel">
          <div className="canvas-panel__toolbar">
            <div className="scope-header">
              <span className="eyebrow">Workflow scope</span>
              <nav className="breadcrumbs" aria-label="Workflow breadcrumbs">
                {controller.state.scopePath.map((scope, index) => (
                  <span className="breadcrumb" key={`${scope.id}-${index}`}>
                    <button
                      className={index === controller.state.scopePath.length - 1 ? 'is-current' : ''}
                      disabled={index === controller.state.scopePath.length - 1}
                      onClick={() => void navigateToScope(index)}
                      type="button"
                    >
                      {scope.name}
                    </button>
                    {index < controller.state.scopePath.length - 1 && <span aria-hidden="true">/</span>}
                  </span>
                ))}
              </nav>
              {controller.state.nestedSubgraphs.length > 0 && (
                <div aria-label="Nested subgraphs" className="nested-subgraphs">
                  <span className="eyebrow">Nested</span>
                  {controller.state.nestedSubgraphs.map((subgraph) => (
                    <button
                      aria-label={`Open ${subgraph.name} subgraph`}
                      className="button button--small"
                      key={subgraph.id}
                      onClick={() => openNestedSubgraph(subgraph.id)}
                      title={`${subgraph.name} · ${subgraph.version}`}
                      type="button"
                    >
                      {subgraph.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div className="canvas-panel__status">
              <span className="canvas-panel__meta">
                {controller.state.nodes.length} nodes&nbsp; · &nbsp;{controller.state.edges.length} links
              </span>
              <DependencySummary report={controller.state.dependencyReport} hash={controller.state.workflowHash} />
            </div>
          </div>
          <div className="flow-canvas">
            <ReactFlow
              fitView
              nodes={flowNodes}
              edges={flowEdges}
              nodeTypes={nodeTypes}
              onConnect={onConnect}
              onEdgesChange={onEdgesChange}
              onNodeClick={(_, node: Node) => controller.selectNode(node.id)}
              onNodesChange={onNodesChange}
              onPaneClick={() => controller.selectNode(null)}
              proOptions={{ hideAttribution: true }}
            >
              <Background color="#27303d" gap={22} size={1} />
              <Controls />
              <MiniMap
                nodeColor={(node) => (node.type === 'rawweave' ? '#6ee7c7' : '#596579')}
                pannable
                zoomable
              />
            </ReactFlow>
            {controller.state.nodes.length === 0 && (
              <div className="canvas-empty">
                <span className="canvas-empty__icon">+</span>
                <strong>Start weaving</strong>
                <p>Add a node from the library to build your first graph.</p>
              </div>
            )}
          </div>
          <div className="canvas-panel__footer">
            <div className="status-dot" />
            <span>{controller.state.notification ?? 'Ready'}</span>
            <span className="footer-hint">Drag from an output handle to an input handle</span>
          </div>
        </section>
        <Inspector
          node={selectedNode}
          selectedNodeIds={controller.state.selectedNodeIds}
          workflowInputs={controller.state.workflowInputs}
          workflowOutputs={controller.state.workflowOutputs}
          onChange={onParameterChange}
          onToggleExposed={onToggleExposed}
          onToggleInput={onToggleInput}
          onDelete={(nodeId) => void controller.removeNode(nodeId).catch(() => undefined)}
        />
      </section>

      <Viewer
        controller={viewerController}
        nodes={controller.state.nodes}
        onPaintedMaskChange={onPaintedMaskChange}
        paintedNode={selectedNode?.typeId === 'core.mask-painted' ? selectedNode : undefined}
        revision={controller.state.revision}
        source={controller.state.source}
      />

      {controller.state.error && (
        <div className="error-toast" role="alert">
          <strong>Command failed</strong>
          <span>{controller.state.error}</span>
          <button onClick={() => controller.selectNode(controller.state.selectedNodeId)} type="button">
            ×
          </button>
        </div>
      )}
      {imageError && (
        <div className="error-toast" role="alert">
          <strong>Image open failed</strong>
          <span>{imageError}</span>
          <button onClick={() => setImageError(null)} type="button">
            ×
          </button>
        </div>
      )}
    </main>
  );
}
