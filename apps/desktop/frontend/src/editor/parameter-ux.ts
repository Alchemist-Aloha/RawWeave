import type { ParameterDescriptor, ParameterValue } from './types';
import uxData from './parameter-ux.json';

export const POINT_CURVE_NODES = new Set(uxData.pointCurveNodes);

export interface ParameterUX {
  name: string;
  description: string;
  unit?: string;
  min?: number;
  max?: number;
  step?: number;
  precision?: number;
  factor?: number;
  advanced?: boolean;
  options?: Array<{ value: string; label: string }>;
  recommendedRange?: boolean;
}

const specific = uxData.specific as Record<string, Partial<ParameterUX>>;

const readable = (id: string) => id.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/[_-]+/g, ' ').replace(/\b\w/g, (letter) => letter.toUpperCase());
const commonNames = uxData.commonNames as Record<string, string>;
const commonDescriptions = uxData.commonDescriptions as Record<string, string>;
const advancedIds = new Set(uxData.advancedIds);

export function parameterUX(typeId: string, parameter: ParameterDescriptor): ParameterUX {
  const exact = specific[`${typeId}:${parameter.id}`] ?? specific[`${typeId.split('.')[0]}.${parameter.id}`];
  const name = exact?.name ?? commonNames[parameter.id] ?? parameter.name;
  const nodeName = readable(typeId.split('.').at(-1) ?? 'operation').toLowerCase();
  const description = exact?.description ?? commonDescriptions[parameter.id] ?? `${name} controls the ${readable(parameter.id).toLowerCase()} used by this ${nodeName} node. Increase the value to apply more of this effect; decrease it to apply less. Default: ${String(parameter.default)}.`;
  const numeric = parameter.parameterType === 'Float' || parameter.parameterType === 'Integer';
  return {
    name,
    description,
    ...(numeric && parameter.min != null ? { min: parameter.min } : {}),
    ...(numeric && parameter.max != null ? { max: parameter.max } : {}),
    ...(numeric ? { step: parameter.parameterType === 'Integer' ? 1 : 0.01, precision: parameter.parameterType === 'Integer' ? 0 : 2 } : {}),
    ...(advancedIds.has(parameter.id) || /^m\d{2}$/.test(parameter.id) || parameter.id.startsWith('offset_') ? { advanced: true } : {}),
    ...(exact?.min != null && exact.max != null ? { recommendedRange: true } : {}),
    ...exact,
    ...(parameter.id === 'points' && POINT_CURVE_NODES.has(typeId) ? { advanced: false, description: 'Linear interpolation between x,y pairs separated by semicolons. Input is horizontal; output is vertical. Enter or leave the field to apply; Escape cancels.' } : {}),
  };
}

export function isRecommendedValue(value: ParameterValue, defaultValue: ParameterValue): boolean {
  return value === defaultValue;
}
