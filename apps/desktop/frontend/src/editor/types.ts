export type ParameterType = 'Float' | 'Integer' | 'Boolean' | 'String';
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
  exposedParameters?: string[];
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
  scopePath?: ScopeBreadcrumb[];
  workflowInputs?: WorkflowPort[];
  workflowOutputs?: WorkflowPort[];
  workflowParameters?: WorkflowParameter[];
  nestedSubgraphs?: WorkflowSummary[];
  workflowHash?: string;
  dependencyReport?: DependencyReport;
}

export type WorkflowPortDirection = 'Input' | 'Output';

export interface WorkflowIdentity {
  id: string;
  version: string;
}

export interface WorkflowMetadata {
  name: string;
  author?: string | null;
  description?: string | null;
  thumbnail?: string | null;
  tags?: string[];
  license?: string | null;
  recommendedInputType?: string | null;
  minimumAppVersion?: string | null;
}

export interface WorkflowPort {
  id: string;
  name: string;
  direction: WorkflowPortDirection;
  nodeId: string;
  portId: string;
  dataType: string;
  required: boolean;
}

export interface WorkflowParameter {
  id: string;
  name: string;
  nodeId: string;
  parameterId: string;
  parameterType: ParameterType;
  default: ParameterValue;
}

export interface WorkflowDependency {
  id: string;
  version: string;
  hash?: string;
}

export interface DependencyDiagnostic {
  id: string;
  requiredVersion: string;
  availableVersion?: string | null;
}

export type DependencyStatus =
  | { kind: 'available' }
  | { kind: 'missing' }
  | { kind: 'version-mismatch'; required: string; available: string };

export interface DependencyReport {
  available: DependencyDiagnostic[];
  missing: DependencyDiagnostic[];
  mismatched: DependencyDiagnostic[];
  disabledNodes: string[];
  statuses?: Record<string, DependencyStatus>;
}

export interface WorkflowDefinition {
  identity: WorkflowIdentity;
  graph: PlatformSnapshot;
  parameters: WorkflowParameter[];
  inputs: WorkflowPort[];
  outputs: WorkflowPort[];
  subgraphDependencies: WorkflowDependency[];
  nodePackDependencies: WorkflowDependency[];
  metadata: WorkflowMetadata;
  nestedSubgraphs: Record<string, WorkflowDefinition>;
  hash: string;
}

export interface WorkflowSummary {
  id: string;
  name: string;
  version: string;
  hash: string;
}

export interface ScopeBreadcrumb {
  id: string;
  name: string;
  version?: string;
  hash?: string;
}

export interface CreateSubgraphOptions {
  id: string;
  version: string;
  metadata: WorkflowMetadata;
  nodePackDependencies?: WorkflowDependency[];
  subgraphDependencies?: WorkflowDependency[];
}

export type SourceKind = 'ordinary' | 'raw';

export interface RawMetadataSummary {
  camera: string;
  lens: string | null;
  iso: number | null;
  aperture: number | null;
  shutter: number | null;
  focalLength: number | null;
  captureTime: string | null;
  orientation: string;
  dimensions: { width: number; height: number };
  exif: Record<string, string>;
}

export interface OpenImageResult {
  kind: SourceKind;
  width: number;
  height: number;
  revision: number;
  metadata: RawMetadataSummary | null;
}

export interface EditorPlatform {
  nodeDescriptors(): Promise<NodeDescriptor[]>;
  snapshot(): Promise<PlatformSnapshot>;
  chooseImagePath(): Promise<string | null>;
  openImage(path: string): Promise<OpenImageResult>;
  addNode(nodeId: string, typeId: string): Promise<void>;
  removeNode(nodeId: string): Promise<void>;
  connect(fromNode: string, fromPort: string, toNode: string, toPort: string): Promise<void>;
  disconnect(fromNode: string, fromPort: string, toNode: string, toPort: string): Promise<void>;
  setParameter(nodeId: string, parameterId: string, value: ParameterValue): Promise<void>;
  exposeParameter(nodeId: string, parameterId: string): Promise<void>;
  unexposeParameter(nodeId: string, parameterId: string): Promise<void>;
  exposeInput(nodeId: string, portId: string): Promise<void>;
  exposeOutput(nodeId: string, portId: string): Promise<void>;
  hidePort(portId: string): Promise<void>;
  createSubgraph(selection: string[], options: CreateSubgraphOptions): Promise<WorkflowDefinition>;
  openSubgraph(id: string): Promise<void>;
  returnToParent(): Promise<void>;
  saveBlueprint(): Promise<string>;
  exportBlueprint(): Promise<string>;
  loadBlueprint(serialized: string): Promise<WorkflowDefinition>;
  importBlueprint(serialized: string): Promise<WorkflowDefinition>;
  instantiateBlueprint(serialized: string): Promise<void>;
  dependencyStatus(): Promise<DependencyReport>;
  workflowHash(): Promise<string>;
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
  source: OpenImageResult | null;
  selectedNodeId: string | null;
  selectedNodeIds: string[];
  scopePath: ScopeBreadcrumb[];
  workflowInputs: WorkflowPort[];
  workflowOutputs: WorkflowPort[];
  workflowParameters: WorkflowParameter[];
  nestedSubgraphs: WorkflowSummary[];
  workflowHash: string | null;
  dependencyReport: DependencyReport | null;
  blueprint: WorkflowDefinition | null;
  error: string | null;
  notification: string | null;
}
