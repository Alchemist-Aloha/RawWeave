import type {
  EditorEdge,
  EditorNode,
  EditorPlatform,
  EditorState,
  NodeDescriptor,
  OpenImageResult,
  ParameterValue,
  Position,
  CreateSubgraphOptions,
  WorkflowDefinition,
} from './types';

interface WorkflowDocument {
  version: 1;
  graph: string;
  positions: Record<string, Position>;
}

type Listener = (state: EditorState) => void;

interface HistorySnapshot {
  graph: string;
  positions: Record<string, Position>;
}

const HISTORY_LIMIT = 50;

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function edgeId(fromNode: string, fromPort: string, toNode: string, toPort: string): string {
  return `${fromNode}:${fromPort}->${toNode}:${toPort}`;
}

export class EditorController {
  public state: EditorState = {
    descriptors: [],
    nodes: [],
    edges: [],
    revision: 0,
    source: null,
    selectedNodeId: null,
    selectedNodeIds: [],
    // Matches the identity the backend gives a flat, blueprint-less workflow in
    // `current_or_flat_blueprint`, so batch pins hash a definition the backend agrees with.
    scopePath: [{ id: 'workflow', name: 'Workflow', version: '1.0.0' }],
    workflowInputs: [],
    workflowOutputs: [],
    workflowParameters: [],
    nestedSubgraphs: [],
    workflowHash: null,
    dependencyReport: null,
    blueprint: null,
    canUndo: false,
    canRedo: false,
    error: null,
    notification: null,
  };

  private readonly listeners = new Set<Listener>();
  private readonly positions = new Map<string, Position>();
  private readonly undoHistory: HistorySnapshot[] = [];
  private readonly redoHistory: HistorySnapshot[] = [];
  private blueprintSerialized: string | null = null;
  private currentHistorySnapshot: HistorySnapshot | null = null;
  private commandQueue: Promise<void> = Promise.resolve();
  private pendingPositionHistory: HistorySnapshot | null = null;

  public constructor(private readonly platform: EditorPlatform) {}

  public subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private publish(): void {
    for (const listener of this.listeners) listener(this.state);
  }

  private setState(patch: Partial<EditorState>): void {
    this.state = { ...this.state, ...patch };
    this.publish();
  }

  private cloneHistorySnapshot(snapshot: HistorySnapshot): HistorySnapshot {
    return {
      graph: snapshot.graph,
      positions: Object.fromEntries(
        Object.entries(snapshot.positions).map(([id, position]) => [id, { ...position }]),
      ),
    };
  }

  private async captureHistorySnapshot(): Promise<HistorySnapshot> {
    return {
      graph: await this.platform.saveWorkflow(),
      positions: Object.fromEntries(
        [...this.positions.entries()].map(([id, position]) => [id, { ...position }]),
      ),
    };
  }

  private historyEqual(left: HistorySnapshot, right: HistorySnapshot): boolean {
    return left.graph === right.graph && JSON.stringify(left.positions) === JSON.stringify(right.positions);
  }

  private publishHistoryState(): void {
    this.setState({
      canUndo: this.undoHistory.length > 0,
      canRedo: this.redoHistory.length > 0,
    });
  }

  private clearHistory(): void {
    this.undoHistory.length = 0;
    this.redoHistory.length = 0;
    this.pendingPositionHistory = null;
  }

  private async establishHistoryBaseline(): Promise<void> {
    this.clearHistory();
    this.currentHistorySnapshot = await this.captureHistorySnapshot();
    this.publishHistoryState();
  }

  private pushHistory(before: HistorySnapshot, after: HistorySnapshot): void {
    if (this.historyEqual(before, after)) {
      this.currentHistorySnapshot = this.cloneHistorySnapshot(after);
      return;
    }
    this.undoHistory.push(this.cloneHistorySnapshot(before));
    if (this.undoHistory.length > HISTORY_LIMIT) this.undoHistory.shift();
    this.redoHistory.length = 0;
    this.currentHistorySnapshot = this.cloneHistorySnapshot(after);
    this.publishHistoryState();
  }

  private commitPendingPositionHistory(): void {
    if (!this.pendingPositionHistory || !this.currentHistorySnapshot) return;
    const before = this.pendingPositionHistory;
    const after: HistorySnapshot = {
      graph: this.currentHistorySnapshot.graph,
      positions: Object.fromEntries(
        [...this.positions.entries()].map(([id, position]) => [id, { ...position }]),
      ),
    };
    this.pendingPositionHistory = null;
    this.pushHistory(before, after);
  }

  public async initialize(): Promise<void> {
    try {
      const descriptors = await this.platform.nodeDescriptors();
      this.setState({ descriptors, error: null });
      await this.refresh();
      await this.establishHistoryBaseline();
    } catch (error) {
      this.setState({ error: errorMessage(error) });
      throw error;
    }
  }

  public async refreshDescriptors(): Promise<void> {
    try {
      const descriptors = await this.platform.nodeDescriptors();
      this.setState({ descriptors, error: null });
      await this.refresh();
    } catch (error) {
      this.setState({ error: errorMessage(error) });
      throw error;
    }
  }

  private async refresh(loadedPositions?: Record<string, Position>): Promise<void> {
    const snapshot = await this.platform.snapshot();
    const descriptors = this.state.descriptors;
    if (loadedPositions) {
      this.positions.clear();
      for (const [id, position] of Object.entries(loadedPositions)) this.positions.set(id, position);
    }
    const nodes: EditorNode[] = snapshot.nodes.map((node, index) => {
      const descriptor = descriptors.find((candidate) => candidate.typeId === node.typeId);
      if (!descriptor) throw new Error(`node type '${node.typeId}' is not registered`);
      const position = this.positions.get(node.id) ?? {
        x: 80 + (index % 3) * 280,
        y: 80 + Math.floor(index / 3) * 180,
      };
      this.positions.set(node.id, position);
      return { ...node, descriptor, position };
    });
    const knownNodeIds = new Set(nodes.map((node) => node.id));
    for (const id of this.positions.keys()) {
      if (!knownNodeIds.has(id)) this.positions.delete(id);
    }
    const edges: EditorEdge[] = snapshot.edges.map((edge) => ({
      ...edge,
      id: edgeId(edge.fromNode, edge.fromPort, edge.toNode, edge.toPort),
      source: edge.fromNode,
      sourceHandle: edge.fromPort,
      target: edge.toNode,
      targetHandle: edge.toPort,
    }));
    const selectedNodeIds = this.state.selectedNodeIds.filter((id) => nodes.some((node) => node.id === id));
    const selectedNodeId = selectedNodeIds.includes(this.state.selectedNodeId ?? '')
      ? this.state.selectedNodeId
      : selectedNodeIds.at(-1) ?? null;
    const dependencyReport = snapshot.dependencyReport ?? (await this.platform.dependencyStatus());
    const workflowHash = snapshot.workflowHash ?? (await this.platform.workflowHash());
    this.setState({
      nodes,
      edges,
      revision: snapshot.revision ?? this.state.revision,
      selectedNodeId,
      selectedNodeIds,
      scopePath: snapshot.scopePath ?? this.state.scopePath,
      workflowInputs: snapshot.workflowInputs ?? [],
      workflowOutputs: snapshot.workflowOutputs ?? [],
      workflowParameters: snapshot.workflowParameters ?? [],
      nestedSubgraphs: snapshot.nestedSubgraphs ?? [],
      workflowHash,
      dependencyReport,
    });
  }

  private enqueue(action: () => Promise<void>): Promise<void> {
    const next = this.commandQueue.then(action);
    this.commandQueue = next.catch(() => undefined);
    return next;
  }

  private command(action: () => Promise<void>): Promise<void> {
    return this.enqueue(() => this.performCommand(action));
  }

  private async performCommand(action: () => Promise<void>): Promise<void> {
    try {
      this.commitPendingPositionHistory();
      const before = this.currentHistorySnapshot
        ? this.cloneHistorySnapshot(this.currentHistorySnapshot)
        : await this.captureHistorySnapshot();
      await action();
      await this.refresh();
      const after = await this.captureHistorySnapshot();
      this.pushHistory(before, after);
      this.setState({ error: null });
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  private async restoreHistorySnapshot(snapshot: HistorySnapshot): Promise<void> {
    await this.platform.restoreWorkflowHistory(snapshot.graph);
    await this.refresh(snapshot.positions);
    this.currentHistorySnapshot = this.cloneHistorySnapshot(snapshot);
  }

  public undo(): Promise<void> {
    return this.enqueue(() => this.undoNow());
  }

  private async undoNow(): Promise<void> {
    this.commitPendingPositionHistory();
    const target = this.undoHistory.pop();
    if (!target) return;
    const current = this.currentHistorySnapshot
      ? this.cloneHistorySnapshot(this.currentHistorySnapshot)
      : await this.captureHistorySnapshot();
    try {
      await this.restoreHistorySnapshot(target);
      this.redoHistory.push(current);
      if (this.redoHistory.length > HISTORY_LIMIT) this.redoHistory.shift();
      this.publishHistoryState();
      this.setState({ error: null, notification: 'Undid last graph edit' });
    } catch (error) {
      this.undoHistory.push(target);
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public redo(): Promise<void> {
    return this.enqueue(() => this.redoNow());
  }

  private async redoNow(): Promise<void> {
    this.commitPendingPositionHistory();
    const target = this.redoHistory.pop();
    if (!target) return;
    const current = this.currentHistorySnapshot
      ? this.cloneHistorySnapshot(this.currentHistorySnapshot)
      : await this.captureHistorySnapshot();
    try {
      await this.restoreHistorySnapshot(target);
      this.undoHistory.push(current);
      if (this.undoHistory.length > HISTORY_LIMIT) this.undoHistory.shift();
      this.publishHistoryState();
      this.setState({ error: null, notification: 'Redid graph edit' });
    } catch (error) {
      this.redoHistory.push(target);
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async openImage(path: string): Promise<OpenImageResult> {
    try {
      const result = await this.platform.openImage(path);
      await this.refresh();
      await this.establishHistoryBaseline();
      this.setState({
        source: result,
        error: null,
        notification:
          result.kind === 'raw' ? 'RAW source opened; display transform is ready to preview' : 'Image opened',
      });
      return result;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async openImageSet(paths: string[], order: import('../imageset/model').ImageSetOrder): Promise<import('./types').OpenImageSetResult> {
    try {
      const result = await this.platform.openImageSet(paths, order);
      await this.refresh();
      await this.establishHistoryBaseline();
      this.setState({
        source: result,
        error: null,
        notification: `ImageSet loaded (${result.members.length} members)`,
      });
      return result;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async createNode(typeId: string, requestedId?: string): Promise<string> {
    const descriptor = this.state.descriptors.find((candidate) => candidate.typeId === typeId);
    if (!descriptor) {
      const error = new Error(`node type '${typeId}' is not registered`);
      this.setState({ error: error.message });
      throw error;
    }
    const baseId = requestedId ?? typeId.split('.').at(-1) ?? 'node';
    const existing = new Set(this.state.nodes.map((node) => node.id));
    let nodeId = baseId;
    let suffix = 2;
    while (existing.has(nodeId)) nodeId = `${baseId}-${suffix++}`;
    await this.command(() => this.platform.addNode(nodeId, typeId));
    this.setState({ selectedNodeId: nodeId, notification: `${descriptor.name} added` });
    return nodeId;
  }

  public async removeNode(nodeId: string): Promise<void> {
    await this.command(() => this.platform.removeNode(nodeId));
    if (this.state.selectedNodeId === nodeId) this.setState({ selectedNodeId: null });
  }

  public async connect(
    fromNode: string,
    fromPort: string,
    toNode: string,
    toPort: string,
  ): Promise<void> {
    await this.command(() => this.platform.connect(fromNode, fromPort, toNode, toPort));
  }

  public async disconnect(
    fromNode: string,
    fromPort: string,
    toNode: string,
    toPort: string,
  ): Promise<void> {
    await this.command(() => this.platform.disconnect(fromNode, fromPort, toNode, toPort));
  }

  public async setParameter(nodeId: string, parameterId: string, value: ParameterValue): Promise<void> {
    await this.command(() => this.platform.setParameter(nodeId, parameterId, value));
  }

  /** A single user gesture publishes once and creates one undo entry. */
  public async setParameters(nodeId: string, values: Record<string, ParameterValue>): Promise<void> {
    await this.command(async () => {
      const node = this.state.nodes.find((node) => node.id === nodeId);
      if (!node) throw new Error(`node '${nodeId}' does not exist`);
      const previous = Object.entries(values).map(([id]) => {
        const descriptor = node.descriptor.parameters.find((parameter) => parameter.id === id);
        if (!descriptor) throw new Error(`parameter '${id}' does not exist`);
        return [id, node.parameters[id] ?? descriptor.default] as const;
      });
      try {
        for (const [id, value] of Object.entries(values)) {
          await this.platform.setParameter(nodeId, id, value);
        }
      } catch (error) {
        try {
          for (const [id, value] of previous.reverse()) {
            await this.platform.setParameter(nodeId, id, value);
          }
        } catch (restoreError) {
          throw new Error(`Parameter edit failed: ${errorMessage(error)}. Restoring previous values failed: ${errorMessage(restoreError)}`);
        } finally {
          await this.refresh();
        }
        throw error;
      }
    });
  }

  public async setWorkflowParameter(workflowParameterId: string, value: ParameterValue): Promise<void> {
    const parameter = this.state.workflowParameters.find((candidate) => candidate.id === workflowParameterId);
    if (!parameter) throw new Error(`workflow parameter '${workflowParameterId}' does not exist`);
    await this.setParameter(parameter.nodeId, parameter.parameterId, value);
  }

  public async exposeParameter(nodeId: string, parameterId: string): Promise<void> {
    await this.command(() => this.platform.exposeParameter(nodeId, parameterId));
  }

  public async unexposeParameter(nodeId: string, parameterId: string): Promise<void> {
    await this.command(() => this.platform.unexposeParameter(nodeId, parameterId));
  }

  public async exposeInput(nodeId: string, portId: string): Promise<void> {
    await this.command(() => this.platform.exposeInput(nodeId, portId));
  }

  public async exposeOutput(nodeId: string, portId: string): Promise<void> {
    await this.command(() => this.platform.exposeOutput(nodeId, portId));
  }

  public async hidePort(portId: string): Promise<void> {
    await this.command(() => this.platform.hidePort(portId));
  }

  public async createSubgraphFromSelection(
    selection: string[],
    options: CreateSubgraphOptions,
  ): Promise<WorkflowDefinition> {
    try {
      const definition = await this.platform.createSubgraph(selection, options);
      await this.platform.openSubgraph(definition.identity.id);
      await this.refresh();
      this.setState({
        selectedNodeId: null,
        selectedNodeIds: [],
        blueprint: null,
        notification: `Subgraph '${definition.metadata.name}' created`,
        error: null,
      });
      return definition;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async openSubgraph(id: string): Promise<void> {
    await this.command(() => this.platform.openSubgraph(id));
    this.setState({ selectedNodeId: null, selectedNodeIds: [], notification: `Opened subgraph '${id}'` });
  }

  public async returnToParent(): Promise<void> {
    await this.command(() => this.platform.returnToParent());
    this.setState({ selectedNodeId: null, selectedNodeIds: [], notification: 'Returned to parent scope' });
  }

  public selectNodes(nodeIds: string[]): void {
    const valid = [...new Set(nodeIds)].filter((id) => this.state.nodes.some((node) => node.id === id));
    this.setState({ selectedNodeIds: valid, selectedNodeId: valid.at(-1) ?? null, error: null });
  }

  public selectNode(nodeId: string | null): void {
    this.setState({ selectedNodeId: nodeId, selectedNodeIds: nodeId ? [nodeId] : [], error: null });
  }

  public updateNodePosition(nodeId: string, position: Position, dragging = false): void {
    if (!this.state.nodes.some((node) => node.id === nodeId)) return;
    const current = this.positions.get(nodeId);
    if (current?.x === position.x && current.y === position.y) {
      if (!dragging) this.commitPendingPositionHistory();
      return;
    }
    if (!this.pendingPositionHistory && this.currentHistorySnapshot) {
      this.pendingPositionHistory = this.cloneHistorySnapshot(this.currentHistorySnapshot);
    }
    this.positions.set(nodeId, { ...position });
    // Publish every intermediate position. A controlled React Flow node only
    // moves once the new position comes back through the `nodes` prop, so
    // skipping the notification freezes the node at its old place for the whole
    // gesture and drops it into position on release. The canvas stays cheap
    // because App caches each node's flow object by editor-node identity, so only
    // the dragged node is rebuilt and re-rendered.
    this.setState({
      nodes: this.state.nodes.map((node) => (
        node.id === nodeId ? { ...node, position: { ...position } } : node
      )),
    });
    if (!dragging) this.commitPendingPositionHistory();
  }

  private async serializeWorkflowDocument(): Promise<string> {
    const graph = await this.platform.saveWorkflow();
    const document: WorkflowDocument = {
      version: 1,
      graph,
      positions: Object.fromEntries(this.positions),
    };
    return JSON.stringify(document, null, 2);
  }

  public async serializeWorkflow(): Promise<string> {
    try {
      return await this.serializeWorkflowDocument();
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async saveWorkflow(): Promise<string> {
    try {
      const serialized = await this.serializeWorkflowDocument();
      this.setState({ error: null, notification: 'Workflow saved' });
      return serialized;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async saveBlueprint(): Promise<string> {
    try {
      const serialized = await this.platform.saveBlueprint();
      this.blueprintSerialized = serialized;
      this.setState({ error: null, notification: 'Blueprint saved' });
      return serialized;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async exportBlueprint(): Promise<string> {
    try {
      const serialized = await this.platform.exportBlueprint();
      this.blueprintSerialized = serialized;
      this.setState({ error: null, notification: 'Blueprint exported' });
      return serialized;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async importBlueprint(serialized: string): Promise<WorkflowDefinition> {
    try {
      const blueprint = await this.platform.importBlueprint(serialized);
      this.blueprintSerialized = serialized;
      this.setState({ blueprint, error: null, notification: `Blueprint '${blueprint.metadata.name}' imported` });
      return blueprint;
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async instantiateBlueprint(serialized?: string): Promise<void> {
    try {
      const source = serialized ?? this.blueprintSerialized;
      if (!source) throw new Error('import a blueprint before instantiating it');
      await this.platform.instantiateBlueprint(source);
      await this.refresh();
      this.setState({ error: null, notification: 'Blueprint instantiated' });
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async loadWorkflow(serialized: string): Promise<void> {
    try {
      const parsed = JSON.parse(serialized) as Partial<WorkflowDocument> & { nodes?: unknown[]; edges?: unknown[] };
      const graph = typeof parsed.graph === 'string' ? parsed.graph : serialized;
      const positions = parsed.positions ?? {};
      await this.platform.loadWorkflow(graph);
      await this.refresh(positions);
      this.blueprintSerialized = null;
      const rawWorkflow = this.state.nodes.some((node) => node.typeId.startsWith('raw.'));
      this.setState({
        source: null,
        blueprint: null,
        selectedNodeId: null,
        selectedNodeIds: [],
        error: null,
        notification: rawWorkflow
          ? 'RAW workflow loaded; select the source RAW file again to render previews'
          : 'Workflow loaded; select an image to render previews',
      });
      await this.establishHistoryBaseline();
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }
}
