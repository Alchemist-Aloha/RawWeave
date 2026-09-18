import type {
  EditorEdge,
  EditorNode,
  EditorPlatform,
  EditorState,
  NodeDescriptor,
  ParameterValue,
  Position,
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
    selectedNodeId: null,
    error: null,
    notification: null,
  };

  private readonly listeners = new Set<Listener>();
  private readonly positions = new Map<string, Position>();

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
    const selectedNodeId = nodes.some((node) => node.id === this.state.selectedNodeId)
      ? this.state.selectedNodeId
      : null;
    this.setState({
      nodes,
      edges,
      revision: snapshot.revision ?? this.state.revision,
      selectedNodeId,
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

  public selectNode(nodeId: string | null): void {
    this.setState({ selectedNodeId: nodeId, error: null });
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

  public async loadWorkflow(serialized: string): Promise<void> {
    try {
      const parsed = JSON.parse(serialized) as Partial<WorkflowDocument> & { nodes?: unknown[]; edges?: unknown[] };
      const graph = typeof parsed.graph === 'string' ? parsed.graph : serialized;
      const positions = parsed.positions ?? {};
      await this.platform.loadWorkflow(graph);
      await this.refresh(positions);
      this.setState({ error: null, notification: 'Workflow loaded' });
    } catch (error) {
      this.setState({ error: errorMessage(error), notification: null });
      throw error;
    }
  }
}
