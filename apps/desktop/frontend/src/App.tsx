import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  type Connection,
  type Edge,
  type EdgeChange,
  type Node,
  type NodeChange,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { EditorController } from './editor/controller';
import type { EditorNode, OpenImageResult, ParameterValue, SourceResult, WorkflowMetadata } from './editor/types';
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
import type { ViewerState } from './viewer/types';
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
import { describeOperationError, redactErrorDetails } from './ui/errors';
import { shortcutAction } from './ui/shortcuts';

const nodeTypes = { rawweave: GraphNode };

type ViewerSessionState = {
  layout: ViewerState['layout'];
  targets: BrowserSession['viewer']['targets'];
};

export interface BrowserSessionRestoreActions {
  initializeEditor: () => Promise<void>;
  setImageSets: (collections: ImageSetCollection[]) => void;
  setActiveImageSetId: (activeId: string | null) => void;
  setPanelLayout: (layout: ViewerState['layout']) => void;
  loadWorkflow: (serialized: string) => Promise<void>;
  openImage: (path: string) => Promise<OpenImageResult>;
  setSourceDimensions: (source: OpenImageResult) => void;
  setViewerTargets: (targets: BrowserSession['viewer']['targets']) => void;
}

export async function restoreBrowserSession(
  session: BrowserSession,
  actions: BrowserSessionRestoreActions,
): Promise<void> {
  await actions.initializeEditor();
  actions.setImageSets(session.imageSets);
  actions.setActiveImageSetId(session.activeImageSetId);
  if (session.panelLayout === 'side-by-side' || session.panelLayout === 'split') {
    actions.setPanelLayout(session.panelLayout);
  }
  if (session.workflow.unsavedWorkingCopy) {
    await actions.loadWorkflow(session.workflow.unsavedWorkingCopy);
  }
  const sourcePath = session.queue.currentPath ?? session.browser.selectedPaths.at(-1) ?? null;
  if (!sourcePath) return;
  const source = await actions.openImage(sourcePath);
  actions.setSourceDimensions(source);
  actions.setViewerTargets(session.viewer.targets);
}

function viewerSessionSnapshot(viewerController: ViewerController): ViewerSessionState {
  return {
    layout: viewerController.state.layout,
    targets: {
      A: viewerController.state.panes.A.target
        ? {
            nodeId: viewerController.state.panes.A.target.nodeId,
            outputPort: viewerController.state.panes.A.target.outputPort,
          }
        : null,
      B: viewerController.state.panes.B.target
        ? {
            nodeId: viewerController.state.panes.B.target.nodeId,
            outputPort: viewerController.state.panes.B.target.outputPort,
          }
        : null,
    },
  };
}

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

export interface EditorErrorContext {
  kind: 'editor' | 'image';
  nodeLabel?: string;
  dependencyIssue?: boolean;
}

export interface EditorErrorNotice {
  title: string;
  message: string;
  guidance: string;
}

export function compatibleDataTypesForNode(node: EditorNode | undefined): string[] {
  if (!node) return [];
  return [...new Set(node.descriptor.outputs.map((output) => output.dataType))];
}

export function describeEditorError(message: string, context: EditorErrorContext): EditorErrorNotice {
  const safeMessage = redactErrorDetails(message).trim() || 'The operation did not complete.';
  if (context.kind === 'image') {
    return {
      title: 'Image source could not be opened',
      message: safeMessage,
      guidance: 'Check that the file is readable and supported, then retry the image operation.',
    };
  }
  const nodePrefix = context.nodeLabel ? `${context.nodeLabel}: ` : '';
  return {
    title: 'Editor operation failed',
    message: `${nodePrefix}${safeMessage}`,
    guidance: context.dependencyIssue
      ? 'Check the node dependencies or AI provider configuration, then retry the operation.'
      : 'Review the node inputs and configuration, then retry the operation.',
  };
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
  const [workspaceMode, setWorkspaceMode] = useState<'build' | 'browse' | 'batch' | 'integrations'>('build');
  const [platform] = useState(() => createPlatform());
  const [batchPlatform] = useState(() => createBatchPlatform());
  const [controller] = useState(() => new EditorController(platform));
  const [hostManager] = useState(() => createTauriHostManager());
  const [viewerController] = useState(() => new ViewerController(createPreviewTransport()));
  const [checkpointPlatform] = useState(() => createCheckpointPlatform());
  const [checkpointController] = useState(() => new CheckpointController(checkpointPlatform));
  const [, setRevision] = useState(0);
  const [, setCheckpointRevision] = useState(0);
  const [viewerSession, setViewerSession] = useState<ViewerSessionState>(() => viewerSessionSnapshot(viewerController));
  const [unsavedWorkflowWorkingCopy, setUnsavedWorkflowWorkingCopy] = useState<string | null>(null);
  const [imageSets, setImageSets] = useState<ImageSetCollection[]>([]);
  const [activeImageSetId, setActiveImageSetId] = useState<string | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [showSubgraphForm, setShowSubgraphForm] = useState(false);
  const [showShortcuts, setShowShortcuts] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const blueprintInput = useRef<HTMLInputElement>(null);
  const imageInput = useRef<HTMLInputElement>(null);
  const nodeSearchInput = useRef<HTMLInputElement>(null);
  const editorInitialization = useRef<Promise<void> | null>(null);
  const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

  const refreshEditorDescriptors = useCallback(
    () => controller.refreshDescriptors(),
    [controller],
  );

  const initializeEditor = useCallback(() => {
    if (!editorInitialization.current) editorInitialization.current = controller.initialize();
    return editorInitialization.current;
  }, [controller]);

  useEffect(() => {
    const unsubscribe = controller.subscribe(() => setRevision((revision) => revision + 1));
    void initializeEditor().catch(() => undefined);
    return unsubscribe;
  }, [controller, initializeEditor]);

  useEffect(() => viewerController.subscribeSession(() => {
    setViewerSession(viewerSessionSnapshot(viewerController));
  }), [viewerController]);

  useEffect(() => {
    if (controller.state.descriptors.length === 0) return;
    let cancelled = false;
    const timeout = window.setTimeout(() => {
      void controller.serializeWorkflow()
        .then((serialized) => {
          if (!cancelled) setUnsavedWorkflowWorkingCopy(serialized);
        })
        .catch(() => undefined);
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(timeout);
    };
  }, [
    controller,
    controller.state.edges,
    controller.state.nodes,
    controller.state.revision,
    controller.state.scopePath,
    controller.state.workflowInputs,
    controller.state.workflowOutputs,
    controller.state.workflowParameters,
  ]);

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
  const compatibleDataTypes = compatibleDataTypesForNode(selectedNode);
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
    (changes: NodeChange[]) => {
      for (const change of changes) {
        if (change.type === 'position' && change.position) {
          controller.updateNodePosition(change.id, change.position, change.dragging ?? false);
        } else if (change.type === 'remove') {
          void controller.removeNode(change.id).catch(() => undefined);
        }
      }
    },
    [controller],
  );

  const onEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
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

  const onBrowserSessionLoaded = useCallback(async (session: BrowserSession) => {
    await restoreBrowserSession(session, {
      initializeEditor,
      setImageSets,
      setActiveImageSetId,
      setPanelLayout: (layout) => viewerController.setLayout(layout),
      loadWorkflow: (serialized) => controller.loadWorkflow(serialized),
      openImage: (path) => controller.openImage(path),
      setSourceDimensions: (source) => viewerController.setSourceDimensions(source),
      setViewerTargets: (targets) => {
        const options = targetsFor(controller.state.nodes);
        for (const viewer of ['A', 'B'] as const) {
          const savedTarget = targets[viewer];
          const target = savedTarget
            ? options.find((option) => option.nodeId === savedTarget.nodeId && option.outputPort === savedTarget.outputPort) ?? null
            : null;
          const currentTarget = viewerController.state.panes[viewer].target;
          if (currentTarget?.nodeId !== target?.nodeId || currentTarget?.outputPort !== target?.outputPort) {
            viewerController.setTarget(viewer, target);
          }
        }
      },
    });
    setWorkspaceMode('build');
  }, [controller, initializeEditor, setActiveImageSetId, setImageSets, viewerController]);

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

  const persistedViewerTargets = viewerSession.targets;

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
        setWorkspaceMode('build');
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
        setWorkspaceMode('build');
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
      } else if (action === 'undo') {
        event.preventDefault();
        void controller.undo().catch(() => undefined);
      } else if (action === 'redo') {
        event.preventDefault();
        void controller.redo().catch(() => undefined);
      } else if (action === 'toggle-shortcuts') {
        event.preventDefault();
        setShowShortcuts((visible) => !visible);
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

  const editorErrorNotice = controller.state.error
    ? describeEditorError(controller.state.error, {
      dependencyIssue: Boolean(controller.state.dependencyReport
        && (controller.state.dependencyReport.missing.length
          || controller.state.dependencyReport.mismatched.length
          || controller.state.dependencyReport.disabledNodes.length)),
      kind: 'editor',
      nodeLabel: selectedNode?.descriptor.name,
    })
    : null;
  const checkpointErrorNotice = checkpointController.state.error
    ? describeOperationError(checkpointController.state.error, {
      dependencyIssue: Boolean(controller.state.dependencyReport
        && (controller.state.dependencyReport.missing.length
          || controller.state.dependencyReport.mismatched.length
          || controller.state.dependencyReport.disabledNodes.length)),
      nodeId: selectedNode?.id,
      nodeLabel: selectedNode?.descriptor.name,
      operation: 'checkpoint-generate',
      outputPort: selectedCheckpointStatus?.outputPort ?? selectedNode?.descriptor.outputs[0]?.id,
    })
    : null;
  const imageErrorNotice = imageError ? describeEditorError(imageError, { kind: 'image' }) : null;

  return (
    <main className={`app-shell app-shell--${workspaceMode}`}>
      <header className="topbar">
        <div className="brand-mark">
          <span className="brand-mark__glyph">RW</span>
          <span>
            <strong>RawWeave</strong>
            <small>node graph editor</small>
          </span>
        </div>
        <nav aria-label="Workspace mode" className="workspace-modes">
          {(['build', 'browse', 'batch', 'integrations'] as const).map((mode) => (
            <button
              aria-current={workspaceMode === mode ? 'page' : undefined}
              className={workspaceMode === mode ? 'is-active' : ''}
              key={mode}
              onClick={() => setWorkspaceMode(mode)}
              type="button"
            >
              {mode === 'build' ? 'Build / Preview' : mode === 'browse' ? 'Browse / Queue' : mode === 'batch' ? 'Batch' : 'Integrations'}
            </button>
          ))}
        </nav>
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
            disabled={!controller.state.canUndo}
            onClick={() => void controller.undo().catch(() => undefined)}
            type="button"
          >
            Undo
          </button>
          <button
            className="button button--quiet"
            disabled={!controller.state.canRedo}
            onClick={() => void controller.redo().catch(() => undefined)}
            type="button"
          >
            Redo
          </button>
          <details className="topbar__more">
            <summary className="button button--quiet">More</summary>
            <div className="topbar__more-menu">
              <button
                aria-expanded={showShortcuts}
                aria-haspopup="dialog"
                className="button button--quiet"
                onClick={() => setShowShortcuts((visible) => !visible)}
                type="button"
              >
                Shortcuts
              </button>
              <button className="button button--quiet" onClick={() => blueprintInput.current?.click()} type="button">Import blueprint</button>
              <button className="button button--quiet" onClick={() => void blueprintAction('save').catch(() => undefined)} type="button">Save blueprint</button>
              <button className="button button--quiet" onClick={() => void blueprintAction('export').catch(() => undefined)} type="button">Export blueprint</button>
              <button className="button button--quiet" disabled={!controller.state.blueprint} onClick={instantiateBlueprint} type="button">Instantiate</button>
            </div>
          </details>
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
        mode={workspaceMode === 'batch' ? 'batch' : 'browse'}
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
        panelLayout={viewerSession.layout}
        unsavedWorkflowWorkingCopy={unsavedWorkflowWorkingCopy}
        viewerTargets={persistedViewerTargets}
        workflowBinding={browserWorkflowBinding}
        workflowParameters={controller.state.workflowParameters}
      />

      <section className="workspace">
        <NodeLibrary
          compatibleDataTypes={compatibleDataTypes}
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
              onNodeClick={(event, node) => toggleNodeSelection(event, node.id)}
              onNodesChange={onNodesChange}
              onPaneClick={() => controller.selectNode(null)}
              onSelectionChange={onSelectionChange}
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

      {showShortcuts && (
        <div aria-label="Keyboard shortcuts" aria-modal="true" className="shortcut-dialog" role="dialog">
          <div className="shortcut-dialog__heading">
            <div>
              <span className="eyebrow">Editor help</span>
              <strong>Keyboard shortcuts</strong>
            </div>
            <button aria-label="Close keyboard shortcuts" className="icon-button" onClick={() => setShowShortcuts(false)} type="button">×</button>
          </div>
          <dl>
            <div><dt>⌘/Ctrl K</dt><dd>Focus node search</dd></div>
            <div><dt>⌘/Ctrl O</dt><dd>Open workflow</dd></div>
            <div><dt>⌘/Ctrl Shift O</dt><dd>Open image</dd></div>
            <div><dt>⌘/Ctrl S</dt><dd>Save workflow</dd></div>
            <div><dt>⌘/Ctrl Z</dt><dd>Undo graph edit</dd></div>
            <div><dt>⌘/Ctrl Shift Z or Y</dt><dd>Redo graph edit</dd></div>
            <div><dt>Delete / Backspace</dt><dd>Delete selected nodes</dd></div>
            <div><dt>?</dt><dd>Toggle this help</dd></div>
          </dl>
        </div>
      )}
      {editorErrorNotice && (
        <div className="error-toast" role="alert">
          <strong>{editorErrorNotice.title}</strong>
          <span>{editorErrorNotice.message}</span>
          <small>{editorErrorNotice.guidance}</small>
          <button aria-label="Dismiss editor error" onClick={() => controller.selectNode(controller.state.selectedNodeId)} type="button">
            ×
          </button>
        </div>
      )}
      {checkpointErrorNotice && (
        <div className="error-toast" role="alert">
          <strong>{checkpointErrorNotice.title}</strong>
          <span>{checkpointErrorNotice.message}</span>
          <small>{checkpointErrorNotice.guidance}</small>
          {selectedNode && (
            <button
              aria-label={checkpointErrorNotice.retryLabel}
              className="error-toast__retry"
              onClick={() => generateCheckpoint(selectedCheckpointStatus?.outputPort)}
              type="button"
            >
              Retry checkpoint
            </button>
          )}
          <button aria-label="Dismiss checkpoint error" onClick={() => checkpointController.clearError()} type="button">
            ×
          </button>
        </div>
      )}
      {imageErrorNotice && (
        <div className="error-toast" role="alert">
          <strong>{imageErrorNotice.title}</strong>
          <span>{imageErrorNotice.message}</span>
          <small>{imageErrorNotice.guidance}</small>
          <button aria-label="Dismiss image error" onClick={() => setImageError(null)} type="button">
            ×
          </button>
        </div>
      )}
    </main>
  );
}
