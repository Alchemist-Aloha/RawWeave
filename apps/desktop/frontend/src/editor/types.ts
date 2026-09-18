export type ParameterType = 'Float' | 'Boolean' | 'String';
export type ParameterValue = number | boolean | string;

export interface PortDescriptor {
  id: string;
  name: string;
  dataType: string;
  required: boolean;
}

export interface ParameterDescriptor {
  id: string;
  name: string;
  parameterType: ParameterType;
  default: ParameterValue;
  min: number | null;
  max: number | null;
}

export interface NodeDescriptor {
  typeId: string;
  name: string;
  version: number;
  inputs: PortDescriptor[];
  outputs: PortDescriptor[];
  parameters: ParameterDescriptor[];
  capabilities?: ExecutionCapability[];
}

export type ExecutionCapability = 'CPU' | 'GPU' | 'TileLocal' | 'RegionAware' | 'FullFrame';

export interface PlatformNode {
  id: string;
  typeId: string;
  parameters: Record<string, ParameterValue>;
}

export interface PlatformEdge {
  fromNode: string;
  fromPort: string;
  toNode: string;
  toPort: string;
}

export interface PlatformSnapshot {
  nodes: PlatformNode[];
  edges: PlatformEdge[];
  revision?: number;
}

export interface OpenImageResult {
  width: number;
  height: number;
  revision: number;
}

export interface EditorPlatform {
  nodeDescriptors(): Promise<NodeDescriptor[]>;
  snapshot(): Promise<PlatformSnapshot>;
  openImage(path: string): Promise<OpenImageResult>;
  addNode(nodeId: string, typeId: string): Promise<void>;
  removeNode(nodeId: string): Promise<void>;
  connect(fromNode: string, fromPort: string, toNode: string, toPort: string): Promise<void>;
  disconnect(fromNode: string, fromPort: string, toNode: string, toPort: string): Promise<void>;
  setParameter(nodeId: string, parameterId: string, value: ParameterValue): Promise<void>;
  saveWorkflow(): Promise<string>;
  loadWorkflow(serialized: string): Promise<void>;
}

export interface Position {
  x: number;
  y: number;
}

export interface EditorNode extends PlatformNode {
  descriptor: NodeDescriptor;
  position: Position;
}

export interface EditorEdge extends PlatformEdge {
  id: string;
  source: string;
  sourceHandle: string;
  target: string;
  targetHandle: string;
}

export interface EditorState {
  descriptors: NodeDescriptor[];
  nodes: EditorNode[];
  edges: EditorEdge[];
  revision: number;
  selectedNodeId: string | null;
  error: string | null;
  notification: string | null;
}
