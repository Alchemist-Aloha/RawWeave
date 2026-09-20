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
import type { ParameterValue, SourceResult, WorkflowMetadata } from './editor/types';
import { createPlatform } from './platform/editor';
import { GraphNode, type RawWeaveFlowNode } from './components/GraphNode';
import { Inspector } from './components/Inspector';
import type {
  CheckpointOutputPort,
  CheckpointPreviewActions,
} from './components/CheckpointPanel';
import { NodeLibrary } from './components/NodeLibrary';
import { targetsFor, Viewer } from './components/Viewer';
import { ViewerController } from './viewer/controller';
import { createPreviewTransport } from './platform/preview';
import { BrowserQueue } from './browser/BrowserQueue';
import type { BrowserSession } from './browser/types';
import type { ImageSetCollection, ImageSetOrder } from './imageset/model';
import { reorderImageSetMembers, setImageSetAlignment } from './imageset/model';
import type { BatchWorkflowContext } from './batch/model';
import { createBatchPlatform } from './platform/batch';
import { HostManager } from './components/HostManager';
import { AiProviderManager } from './components/AiProviderManager';
import { createTauriHostManager } from './platform/hosts';
import { CheckpointController } from './checkpoint/controller';
import { createCheckpointPlatform } from './platform/checkpoint';
import { shortcutAction } from './ui/shortcuts';

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

function SourceMetadata({ source }: { source: SourceResult | null }) {
  if (!source) return null;
  if (source.kind === 'imageset') {
    const metadata = source.sharedMetadata;
    const rows = [
      ['Members', source.members.length],
      ['Order', source.order],
      ['Alignment', source.alignment.state === 'aligned' ? 'Aligned' : 'Unaligned'],
      ['Camera', metadata?.camera],
      ['Lens', metadata?.lens],
      ['ISO', metadata?.iso],
      ['Capture time', metadata?.captureTime],
      ['Dimensions', metadata ? `${metadata.dimensions.width} × ${metadata.dimensions.height}` : null],
    ];
    return (
      <section aria-label="Source metadata" className="source-metadata">
        <div>
          <span className="eyebrow">Source</span>
          <strong>ImageSet</strong>
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
  const [checkpointPlatform] = useState(() => createCheckpointPlatform());
  const [checkpointController] = useState(() => new CheckpointController(checkpointPlatform));
  const [, setRevision] = useState(0);
  const [, setViewerRevision] = useState(0);
  const [, setCheckpointRevision] = useState(0);
  const [restoredSession, setRestoredSession] = useState<BrowserSession | null>(null);
  const [imageSets, setImageSets] = useState<ImageSetCollection[]>([]);
  const [activeImageSetId, setActiveImageSetId] = useState<string | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [showSubgraphForm, setShowSubgraphForm] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const blueprintInput = useRef<HTMLInputElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const nodeSearchInput = useRef<HTMLInputElement>(null);
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

  useEffect(() => {
    const unsubscribe = checkpointController.subscribe(() => setCheckpointRevision((revision) => revision + 1));
    void checkpointController.refresh().catch(() => undefined);
    return () => {
      unsubscribe();
      checkpointController.dispose();
    };
  }, [checkpointController]);

  const flowNodes = useMemo<RawWeaveFlowNode[]>(
    () =>
      controller.state.nodes.map((node) => ({
        id: node.id,
        type: 'rawweave',
        position: node.position,
        data: {
          node,
          checkpointStatus: checkpointController.state.statuses.find((status) => status.nodeId === node.id) ?? null,
        },
        selected: controller.state.selectedNodeIds.includes(node.id),
      })),
    [checkpointController, checkpointController.state.statuses, controller, controller.state.nodes, controller.state.selectedNodeIds],
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
  const selectedCheckpointStatus = selectedNode
    ? checkpointController.state.statuses.find((status) => status.nodeId === selectedNode.id) ?? null
    : null;

  const checkpointPreviewActions = useMemo<CheckpointPreviewActions | undefined>(() => {
    if (!selectedNode || selectedNode.descriptor.evaluationPolicy !== 'manual_checkpoint') return undefined;
    const targets = targetsFor(controller.state.nodes);
    const outputPort = selectedCheckpointStatus?.outputPort ?? selectedNode.descriptor.outputs[0]?.id;
    const generatedTarget = outputPort
      ? targets.find((target) => target.nodeId === selectedNode.id && target.outputPort === outputPort) ?? null
      : null;
    const inputTarget = controller.state.edges
      .filter((edge) => edge.target === selectedNode.id)
      .map((edge) => targets.find((target) => target.nodeId === edge.source && target.outputPort === edge.sourceHandle) ?? null)
      .find((target) => target !== null) ?? null;
    const canPreviewCommitted = Boolean(
      generatedTarget
      && selectedCheckpointStatus?.committedArtifactId
      && selectedCheckpointStatus.canUseCommitted,
    );
    const showInput = () => {
      if (inputTarget) viewerController.setTarget('A', inputTarget);
    };
    const showGenerated = () => {
      if (canPreviewCommitted && generatedTarget) viewerController.setTarget('B', generatedTarget);
    };
    const showDifference = () => {
      if (!inputTarget || !canPreviewCommitted || !generatedTarget) return;
      viewerController.setLayout('side-by-side');
      viewerController.setTarget('A', inputTarget);
      viewerController.setTarget('B', generatedTarget);
    };
    return {
      input: inputTarget ? { onClick: showInput } : undefined,
      generated: canPreviewCommitted ? { onClick: showGenerated } : undefined,
      difference: inputTarget && canPreviewCommitted ? { onClick: showDifference } : undefined,
    };
  }, [controller.state.edges, controller.state.nodes, selectedCheckpointStatus, selectedNode, viewerController]);

  useEffect(() => {
    if (selectedNode?.descriptor.evaluationPolicy !== 'manual_checkpoint') return;
    void checkpointController.refresh(selectedNode.id).catch(() => undefined);
  }, [checkpointController, controller.state.revision, selectedNode?.descriptor.evaluationPolicy, selectedNode?.id]);

  const checkpointOutputPorts = useMemo<CheckpointOutputPort[]>(
    () => selectedNode?.descriptor.outputs.map((output) => ({
      id: output.id,
      name: output.name,
      dataType: output.dataType,
    })) ?? [],
    [selectedNode],
  );

  const generateCheckpoint = useCallback((requestedOutputPort?: string) => {
    const outputPort = requestedOutputPort ?? selectedCheckpointStatus?.outputPort ?? selectedNode?.descriptor.outputs[0]?.id;
    if (!selectedNode || !outputPort) return;
    void checkpointController.generate(selectedNode.id, outputPort).catch(() => undefined);
  }, [checkpointController, selectedCheckpointStatus?.outputPort, selectedNode]);

  const cancelCheckpoint = useCallback(() => {
    if (!selectedNode) return;
    void checkpointController.cancel(selectedNode.id).catch(() => undefined);
  }, [checkpointController, selectedNode]);
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
      // The active snapshot exposes nested scopes as summaries. Keep batch
      // creation conservative until their full definitions are pinned.
      containsManualCheckpoints: controller.state.nodes.some(
        (node) => node.descriptor.evaluationPolicy === 'manual_checkpoint',
      ) || controller.state.nestedSubgraphs.length > 0,
    };
  }, [browserWorkflowBinding, controller.state.blueprint, controller.state.edges, controller.state.nodes, controller.state.nestedSubgraphs, controller.state.revision, controller.state.scopePath, controller.state.workflowInputs, controller.state.workflowOutputs, controller.state.workflowParameters]);

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
    setImageSets(session.imageSets);
    setActiveImageSetId(session.activeImageSetId);
    if (session.panelLayout === 'side-by-side' || session.panelLayout === 'split') {
      viewerController.setLayout(session.panelLayout);
    }
  }, [viewerController]);

  const onImageSetsChange = useCallback((collections: ImageSetCollection[], activeId: string | null) => {
    setImageSets(collections);
    setActiveImageSetId(activeId);
  }, []);

  const activeImageSet = imageSets.find((collection) => collection.id === activeImageSetId) ?? null;

  const reorderActiveImageSetMember = useCallback((memberId: string, targetIndex: number) => {
    if (!activeImageSet) return;
    onImageSetsChange(imageSets.map((collection) => collection.id === activeImageSet.id
      ? reorderImageSetMembers(collection, memberId, targetIndex)
      : collection), activeImageSet.id);
  }, [activeImageSet, imageSets, onImageSetsChange]);

  const setActiveImageSetReference = useCallback((referenceMember: string | null) => {
    if (!activeImageSet) return;
    try {
      const updated = setImageSetAlignment(activeImageSet, referenceMember);
      onImageSetsChange(imageSets.map((collection) => collection.id === updated.id ? updated : collection), updated.id);
    } catch (error) {
      setImageError(error instanceof Error ? error.message : String(error));
    }
  }, [activeImageSet, imageSets, onImageSetsChange]);

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

  const openImageSetPaths = useCallback(
    async (paths: string[], order: ImageSetOrder) => {
      try {
        const imageSet = await controller.openImageSet(paths, order);
        const firstMember = imageSet.members[0];
        if (firstMember) viewerController.setSourceDimensions({ width: firstMember.width, height: firstMember.height });
        setImageError(null);
      } catch (error) {
        setImageError(error instanceof Error ? error.message : String(error));
        throw error;
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

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const action = shortcutAction(event);
      if (!action) return;
      const target = event.target as HTMLElement | null;
      const editingText = typeof target?.closest === 'function'
        && target.closest('input, textarea, select, [contenteditable="true"]');
      if (editingText && !event.ctrlKey && !event.metaKey) return;
      if (action === 'focus-node-search') {
        event.preventDefault();
        nodeSearchInput.current?.focus();
      } else if (action === 'open-image') {
        event.preventDefault();
        void openImage();
      } else if (action === 'open-workflow') {
        event.preventDefault();
        fileInput.current?.click();
      } else if (action === 'save-workflow') {
        event.preventDefault();
        void controller.saveWorkflow().then(downloadWorkflow).catch(() => undefined);
      } else if (action === 'delete-selection' && controller.state.selectedNodeIds.length > 0) {
        event.preventDefault();
        const selectedNodeIds = [...controller.state.selectedNodeIds];
        void (async () => {
          for (const nodeId of selectedNodeIds) await controller.removeNode(nodeId);
        })().catch(() => undefined);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [controller, fileInput, nodeSearchInput, openImage]);

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

      <section className="integration-strip">
        <HostManager api={hostManager} onDiscovery={refreshEditorDescriptors} />
        <AiProviderManager />
      </section>

      <SourceMetadata source={controller.state.source} />
      <BrowserQueue
        activeImageSetId={activeImageSetId}
        batchPlatform={batchPlatform}
        batchWorkflow={batchWorkflow}
        imageSets={imageSets}
        onImageSetsChange={onImageSetsChange}
        onOpenImageSet={openImageSetPaths}
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
          searchInputRef={nodeSearchInput}
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
          imageSet={activeImageSet}
          onImageSetAlignmentChange={setActiveImageSetReference}
          onImageSetReorder={reorderActiveImageSetMember}
          node={selectedNode}
          selectedNodeIds={controller.state.selectedNodeIds}
          workflowInputs={controller.state.workflowInputs}
          workflowOutputs={controller.state.workflowOutputs}
          onChange={onParameterChange}
          onToggleExposed={onToggleExposed}
          onToggleInput={onToggleInput}
          onDelete={(nodeId) => void controller.removeNode(nodeId).catch(() => undefined)}
          checkpointLoading={checkpointController.state.loading}
          checkpointOutputPorts={checkpointOutputPorts}
          checkpointStatus={selectedCheckpointStatus}
          onCancelCheckpoint={cancelCheckpoint}
          onGenerateCheckpoint={generateCheckpoint}
          checkpointPreviewActions={checkpointPreviewActions}
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
      {checkpointController.state.error && (
        <div className="error-toast" role="alert">
          <strong>Checkpoint operation failed</strong>
          <span>{checkpointController.state.error}</span>
          <button aria-label="Dismiss checkpoint error" onClick={() => checkpointController.clearError()} type="button">
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
