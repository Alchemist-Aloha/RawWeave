export type OperationKind = 'checkpoint-generate' | 'viewer-render';

export interface OperationErrorContext {
  operation: OperationKind;
  nodeId?: string;
  nodeLabel?: string;
  outputPort?: string;
  imageLabel?: string;
  dependencyIssue?: boolean;
}

export interface OperationErrorNotice {
  title: string;
  message: string;
  guidance: string;
  retryLabel: string;
}

/** Remove credentials and credential-shaped values before an error reaches the UI. */
export function redactErrorDetails(message: string): string {
  return message
    .replace(/(api[_-]?key|token|secret|password|authorization|credential)\s*[:=]\s*["']?[^,;\s"']+/gi, '$1=[redacted]')
    .replace(/\bBearer\s+[A-Za-z0-9._~+/=-]+/gi, 'Bearer [redacted]')
    .replace(/\b(?:sk|rk|pk)-[A-Za-z0-9_-]{8,}\b/g, '[redacted-key]')
    .replace(/\*{3,}/g, '[redacted]');
}

function operationDetails(context: OperationErrorContext): string {
  return [context.nodeLabel, context.nodeId, context.outputPort, context.imageLabel]
    .filter((value): value is string => Boolean(value && value.trim()))
    .join(' · ');
}

export function describeOperationError(
  message: string,
  context: OperationErrorContext,
): OperationErrorNotice {
  const safeMessage = redactErrorDetails(message).trim() || 'The operation did not complete.';
  const details = operationDetails(context);
  const contextualMessage = details ? `${details}: ${safeMessage}` : safeMessage;
  const isCheckpoint = context.operation === 'checkpoint-generate';
  return {
    title: isCheckpoint ? 'Checkpoint generation failed' : 'Viewer render failed',
    message: contextualMessage,
    guidance: context.dependencyIssue
      ? 'Check the node dependencies or provider configuration, then retry the operation.'
      : isCheckpoint
        ? 'Check the checkpoint inputs and provider configuration, then retry generation.'
        : 'Check the preview provider and node dependencies, then retry the render.',
    retryLabel: isCheckpoint ? 'Retry checkpoint' : 'Retry render',
  };
}
