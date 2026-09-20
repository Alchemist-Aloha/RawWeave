import type { ExecutionCapability, NodeDescriptor, ParameterDescriptor, PortDescriptor } from '../editor/types';

const capabilities: ExecutionCapability[] = ['CPU', 'FullFrame'];
const image = { id: 'image', name: 'Image', dataType: 'core.Image', required: true } satisfies PortDescriptor;
const mask = { id: 'mask', name: 'Mask', dataType: 'core.Mask', required: true } satisfies PortDescriptor;
const output = { id: 'image', name: 'Image', dataType: 'core.Image', required: false } satisfies PortDescriptor;

function stringParameter(id: string, name: string, defaultValue: string): ParameterDescriptor {
  return { id, name, parameterType: 'String', default: defaultValue, min: null, max: null };
}

function integerParameter(id: string, name: string, defaultValue: number, min: number | null = null, max: number | null = null): ParameterDescriptor {
  return { id, name, parameterType: 'Integer', default: defaultValue, min, max };
}

function floatParameter(id: string, name: string, defaultValue: number, min: number | null = null, max: number | null = null): ParameterDescriptor {
  return { id, name, parameterType: 'Float', default: defaultValue, min, max };
}

const sharedParameters: ParameterDescriptor[] = [
  stringParameter('provider_id', 'Provider', 'comfyui'),
  stringParameter('workflow_id', 'Workflow', ''),
  stringParameter('prompt', 'Prompt', ''),
  stringParameter('negative_prompt', 'Negative prompt', ''),
  floatParameter('strength', 'Strength', 0.75, 0, 1),
  integerParameter('steps', 'Steps', 20, 1, 200),
  integerParameter('seed', 'Seed', 0),
];

function descriptor(
  typeId: string,
  name: string,
  inputs: PortDescriptor[],
  parameters = sharedParameters,
): NodeDescriptor {
  return {
    typeId,
    name,
    version: 1,
    inputs,
    outputs: [output],
    parameters: parameters.map((parameter) => ({ ...parameter })),
    evaluationPolicy: 'manual_checkpoint',
    capabilities,
  };
}

export const aiNodeDescriptors: NodeDescriptor[] = [
  descriptor('ai.img2img', 'AI Img2Img', [image]),
  descriptor('ai.inpaint', 'AI Inpaint', [image, mask]),
  descriptor('ai.generative-fill', 'AI Generative Fill', [image, mask]),
  descriptor(
    'ai.upscale',
    'AI Upscale',
    [image],
    [...sharedParameters.map((parameter) => ({ ...parameter })), floatParameter('scale', 'Scale', 2, 1, 8)],
  ),
];
