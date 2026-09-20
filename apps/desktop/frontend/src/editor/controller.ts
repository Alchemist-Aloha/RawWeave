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
    scopePath: [{ id: 'root', name: 'Workflow', version: '1.0.0' }],
    workflowInputs: [],
    workflowOutputs: [],
    workflowParameters: [],
    nestedSubgraphs: [],
    workflowHash: null,
    dependencyReport: null,
    blueprint: null,
    error: null,
    notification: null,
  };

  private readonly listeners = new Set<Listener>();
  private readonly positions = new Map<string, Position>();
  private blueprintSerialized: string | null = null;

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

  public async initialize(): Promise<void> {
    try {
      const descriptors = await this.platform.nodeDescriptors();
      this.setState({ descriptors, error: null });
      await this.refresh();
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

  private async command(action: () => Promise<void>): Promise<void> {
    try {
      await action();
      await this.refresh();
      this.setState({ error: null });
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }

  public async openImage(path: string): Promise<OpenImageResult> {
    try {
      const result = await this.platform.openImage(path);
      await this.refresh();
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

  public updateNodePosition(nodeId: string, position: Position): void {
    this.positions.set(nodeId, position);
    this.setState({
      nodes: this.state.nodes.map((node) => (node.id === nodeId ? { ...node, position } : node)),
    });
  }

  public async saveWorkflow(): Promise<string> {
    try {
      const graph = await this.platform.saveWorkflow();
      const document: WorkflowDocument = {
        version: 1,
        graph,
        positions: Object.fromEntries(this.positions),
      };
      const serialized = JSON.stringify(document, null, 2);
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
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }
}
