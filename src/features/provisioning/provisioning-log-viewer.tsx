import { useEffect, useRef, useState } from 'react';
import type { ProvisioningRun } from '@/lib/bindings';
import { Button } from '@/components/ui/button';

export function ProvisioningLogViewer({ run }: { run: ProvisioningRun }) {
  const [autoScroll, setAutoScroll] = useState(true);
  const [cleared, setCleared] = useState('');
  const [message, setMessage] = useState('');
  const bottom = useRef<HTMLDivElement>(null);
  const output = run.logs?.length
    ? run.logs
        .map(
          (entry) =>
            `${entry.observedAt} [${entry.level}] ${entry.stepKey}: ${entry.message}`,
        )
        .join('\n')
    : run.steps
        .filter((step) => step.sanitizedOutput)
        .map((step) => `${step.description}\n${step.sanitizedOutput}`)
        .join('\n\n');
  const visible =
    cleared && output.startsWith(cleared)
      ? output.slice(cleared.length).trimStart()
      : output;
  useEffect(() => {
    if (autoScroll) bottom.current?.scrollIntoView?.({ block: 'nearest' });
  }, [visible, autoScroll]);
  return (
    <details open className="rounded-lg border bg-background p-4">
      <summary className="cursor-pointer text-sm font-medium">
        Installation output
      </summary>
      <p className="my-2 text-xs text-muted-foreground">
        {run.logs?.length
          ? 'Sanitized output · timestamps indicate when RemoteDock observed each message.'
          : 'Legacy sanitized output · individual timestamps unavailable.'}
      </p>
      <div className="my-3 flex flex-wrap items-center gap-3">
        <label className="flex gap-2 text-sm">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(event) => setAutoScroll(event.target.checked)}
          />
          Auto-scroll
        </label>
        <Button
          variant="outline"
          onClick={() => {
            void navigator.clipboard.writeText(visible).then(
              () => setMessage('Logs copied.'),
              () => setMessage('Unable to copy logs.'),
            );
          }}
        >
          Copy logs
        </Button>
        <Button variant="ghost" onClick={() => setCleared(output)}>
          Clear visible logs
        </Button>
      </div>
      <div className="max-h-80 overflow-auto">
        <pre className="whitespace-pre-wrap break-all font-mono text-xs leading-6">
          {visible || 'No visible output.'}
        </pre>
        <div ref={bottom} />
      </div>
      <p role="status" className="text-xs">
        {message}
      </p>
    </details>
  );
}
