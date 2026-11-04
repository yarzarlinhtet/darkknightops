import { readiness } from '@/features/provisioning/readiness';
import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type {
  Preflight,
  ProvisioningPlan,
  ProvisioningRun,
} from '@/lib/bindings';
import { provisioningApi } from '@/lib/ipc';
import { PreflightChecklist } from '@/features/provisioning/preflight-checklist';
import { ProvisioningLogViewer } from '@/features/provisioning/provisioning-log-viewer';
import { Button } from '@/components/ui/button';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogCancel,
} from '@/components/ui/alert-dialog';

export function ProvisioningPanel({
  id,
  lease,
  visible,
  onBusy,
}: {
  id: string;
  lease?: string;
  visible: boolean;
  onBusy: (value: boolean) => void;
}) {
  const client = useQueryClient();
  const preflightQuery = useQuery({
    queryKey: ['provisioning-preflight', id, lease],
    queryFn: () => provisioningApi.preflight(id, lease!),
    enabled: false,
    retry: false,
  });
  const preflight = preflightQuery.data;
  const setPreflight = (value: Preflight) =>
    client.setQueryData(['provisioning-preflight', id, lease], value);
  const [plan, setPlan] = useState<ProvisioningPlan>();
  const [confirm, setConfirm] = useState(false);
  const [selectedRun, setSelectedRun] = useState<string>();
  const [now, setNow] = useState(() => Date.now());
  const [openedAt] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const key = ['provisioning-runs', id];
  const history = useQuery({
    queryKey: key,
    queryFn: async () => {
      const snapshots = await provisioningApi.history(id);
      const cached =
        client.getQueryData<ProvisioningRun[]>(['provisioning-runs', id]) ?? [];
      return [
        ...snapshots.map((run) => {
          const newer = cached.find(
            (old) => old.id === run.id && old.sequence > run.sequence,
          );
          return newer ?? run;
        }),
        ...cached.filter(
          (run) => !snapshots.some((value) => value.id === run.id),
        ),
      ].sort((a, b) => b.startedAt.localeCompare(a.startedAt));
    },
    enabled: visible,
    retry: false,
    refetchInterval: (query) =>
      query.state.data?.some(
        (run) => run.status === 'RUNNING' || run.status === 'READY',
      )
        ? 2000
        : false,
  });
  const latest = history.data?.[0];
  const selected = history.data?.find((run) => run.id === selectedRun);
  const running =
    history.data?.some(
      (run) => run.status === 'RUNNING' || run.status === 'READY',
    ) ?? false;
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const subscription = listen<ProvisioningRun>(
      'provisioning-progress',
      ({ payload }) => {
        if (payload.serverId !== id || disposed) return;
        client.setQueryData<ProvisioningRun[]>(
          ['provisioning-runs', id],
          (previous) => {
            const old = previous?.find((run) => run.id === payload.id);
            if (old && old.sequence >= payload.sequence) return previous;
            return [
              payload,
              ...(previous ?? []).filter((run) => run.id !== payload.id),
            ].sort((a, b) => b.startedAt.localeCompare(a.startedAt));
          },
        );
        if (payload.status !== 'RUNNING')
          void client.invalidateQueries({ queryKey: ['ssh', id] });
      },
    );
    void subscription
      .then(() => {
        if (!disposed && visible)
          void client.invalidateQueries({
            queryKey: ['provisioning-runs', id],
          });
      })
      .catch(() => {
        if (!disposed)
          void client.invalidateQueries({
            queryKey: ['provisioning-runs', id],
          });
      });
    return () => {
      disposed = true;
      void subscription.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [id, client, visible]);
  const inspect = useMutation({
    mutationFn: async () => {
      setPlan(undefined);
      return provisioningApi.preflight(id, lease!);
    },
    onSuccess: (value) => {
      setPreflight(value);
      setPlan(undefined);
    },
  });
  const planning = useMutation({
    mutationFn: (grant: boolean) => provisioningApi.plan(id, lease!, grant),
    onSuccess: (value) => {
      setPlan(value);
      setPreflight(value.preflight);
    },
  });
  const start = useMutation({
    mutationFn: () =>
      provisioningApi.start(id, lease!, plan!.id, plan!.grantAccess),
    onSuccess: async () => {
      setConfirm(false);
      setPlan(undefined);
      await client.invalidateQueries({ queryKey: key });
    },
  });
  const cancel = useMutation({
    mutationFn: () => provisioningApi.cancel(id, latest!.id),
    onSuccess: () => client.invalidateQueries({ queryKey: key }),
  });
  const pending = inspect.isPending || planning.isPending || start.isPending;
  useEffect(() => {
    onBusy(running || pending);
  }, [running, pending, onBusy]);
  useEffect(() => () => onBusy(false), [onBusy]);
  const current = [preflight, latest?.verification]
    .filter((value): value is Preflight => !!value)
    .sort((a, b) => b.checkedAt.localeCompare(a.checkedAt))[0];
  const blocked = plan?.steps.some(
    (step) => step.status === 'BLOCKED' || step.status === 'UNSUPPORTED',
  );
  const errors = [
    history.error,
    inspect.error,
    planning.error,
    start.error,
    cancel.error,
  ];
  return (
    <section
      className="panel space-y-4"
      hidden={!visible}
      aria-label="Server provisioning"
    >
      <h2>Server provisioning</h2>
      {pending && (
        <p role="status" className="animate-pulse text-sm">
          Checking server state… Previous observations remain visible.
        </p>
      )}
      <p className="text-sm text-muted-foreground">
        Inspect the server, review a plan, then explicitly approve changes.
        Docker daemon access is root-equivalent.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={!lease || running || pending}
          onClick={() => inspect.mutate()}
        >
          Run Preflight Checks / Verify Installation
        </Button>
        <Button
          disabled={!lease || running || pending}
          onClick={() => planning.mutate(false)}
        >
          Inspect and plan provisioning
        </Button>
        <Button
          variant="outline"
          disabled={!lease || running || pending}
          onClick={() => planning.mutate(true)}
        >
          Grant Docker access to this SSH user
        </Button>
      </div>
      {errors.map(
        (error, index) =>
          error && (
            <p role="alert" key={index}>
              {error.message}
            </p>
          ),
      )}
      {current && (
        <div className="space-y-2">
          <h3>Server readiness</h3>
          <p className="status-badge">
            {readiness(
              current,
              latest,
              Date.parse(current.checkedAt) < openedAt ||
                now - Date.parse(current.checkedAt) > 300000,
              pending,
            )}
          </p>
          <p>
            {current.facts.osId} {current.facts.versionId} ({current.codename})
            · {current.facts.architecture} · Kernel {current.kernel}
          </p>
          <p>
            Administrative access: {current.privilege}. Package manager:{' '}
            {current.facts.apt ? 'apt' : 'unavailable'}. Systemd:{' '}
            {current.facts.systemd ? 'available' : 'unavailable'}.
          </p>
          <p>
            Docker Engine:{' '}
            {current.serverVersion || current.clientVersion || 'not detected'} ·
            Compose: {current.composeVersion || 'not detected'}
          </p>
          <p>
            Docker service:{' '}
            {current.facts.serviceRunning ? 'running' : 'not running'} ·{' '}
            {current.facts.serviceEnabled ? 'enabled at boot' : 'not enabled'}
          </p>
          <p>
            Docker source: {current.dockerSource}. Docker access:{' '}
            {current.dockerAccess || 'unknown'} · Docker group:{' '}
            {current.dockerGroup ? 'member' : 'not active'}
            {current.pendingLogin ? ' · New login required' : ''}
          </p>
          <p>
            Existing containers: {current.workloads ?? 'unknown'} · Checked{' '}
            {new Date(current.checkedAt).toLocaleString()}
          </p>
          <PreflightChecklist value={current} />
          <h3>Backend diagnostics</h3>
          <ul className="space-y-2">
            {current.checks.map((step) => (
              <li key={step.key}>
                <strong>
                  {step.status === 'SATISFIED'
                    ? 'Passed'
                    : step.status === 'REQUIRED'
                      ? 'Warning'
                      : step.status === 'UNSUPPORTED'
                        ? 'Unsupported'
                        : 'Failed'}
                </strong>
                : {step.description}
              </li>
            ))}
          </ul>
          {current.privilege === 'interactive' && (
            <p role="alert">
              Interactive sudo passwords are unsupported. Configure suitable
              passwordless administrative access or connect as root.
            </p>
          )}
        </div>
      )}
      {plan && (
        <div className="space-y-3">
          <h3>Review provisioning plan</h3>
          <p className="font-mono text-xs break-all">
            Server {plan.serverId} · Plan {plan.id}
          </p>
          <Button
            variant="outline"
            disabled={pending || running}
            onClick={() => setPlan(undefined)}
          >
            Back
          </Button>
          <Button
            variant="outline"
            disabled={pending || running || !lease}
            onClick={() => planning.mutate(plan.grantAccess)}
          >
            Refresh Plan
          </Button>
          {Date.parse(plan.expiresAt) <= now && (
            <p role="alert">Plan expired. Refresh and approve a new plan.</p>
          )}
          <ol className="list-inside list-decimal space-y-2">
            {plan.steps.map((step) => (
              <li key={step.key}>
                {step.description} <strong>{step.status}</strong>
              </li>
            ))}
          </ol>
          <ul>
            {plan.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
          <p className="text-xs">
            Approval expires {new Date(plan.expiresAt).toLocaleString()}. Server
            state is checked again before execution.
          </p>
          <Button
            disabled={
              blocked ||
              running ||
              pending ||
              !lease ||
              Date.parse(plan.expiresAt) <= now
            }
            onClick={() => setConfirm(true)}
          >
            Review confirmation
          </Button>
        </div>
      )}
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogTitle>
            {plan?.grantAccess
              ? 'Grant root-equivalent Docker access?'
              : 'Approve this provisioning plan?'}
          </AlertDialogTitle>
          <AlertDialogDescription className="my-4">
            {plan?.grantAccess
              ? 'Docker group membership gives this SSH user root-equivalent control over this server. RemoteDock will add the authenticated user to the group and verify access using a fresh SSH connection.'
              : 'This changes packages and Docker service state on the selected server. Package installation may start Docker and alter networking. Cancellation waits for package operations to finish safely; no rollback is promised.'}
          </AlertDialogDescription>
          <div className="flex gap-2">
            <AlertDialogCancel asChild>
              <Button variant="outline" disabled={start.isPending}>
                Cancel
              </Button>
            </AlertDialogCancel>
            <Button
              disabled={
                start.isPending ||
                blocked ||
                !plan ||
                Date.parse(plan.expiresAt) <= now
              }
              onClick={() => start.mutate()}
            >
              Approve and execute
            </Button>
          </div>
          {start.error && <p role="alert">{start.error.message}</p>}
        </AlertDialogContent>
      </AlertDialog>
      {latest && (
        <div className="space-y-3" aria-live="polite">
          <h3>
            Last provisioning result:{' '}
            {latest.cancellationRequested && running
              ? 'Cancel Requested'
              : latest.status}
          </h3>
          <p className="text-sm text-muted-foreground">
            Started {new Date(latest.startedAt).toLocaleString()} · Elapsed{' '}
            {Math.max(
              0,
              Math.floor(
                ((latest.finishedAt ? Date.parse(latest.finishedAt) : now) -
                  Date.parse(latest.startedAt)) /
                  1000,
              ),
            )}{' '}
            seconds
          </p>
          <ProvisioningLogViewer key={latest.id} run={latest} />
          <p>
            {
              latest.steps.filter((step) =>
                ['SUCCEEDED', 'SKIPPED'].includes(step.status),
              ).length
            }{' '}
            of {latest.steps.length} steps completed
          </p>
          {latest.cancellationRequested && (
            <p>Cancellation requested. Waiting for a safe stopping point.</p>
          )}
          {latest.errorMessage && <p role="alert">{latest.errorMessage}</p>}
          {latest.finishedAt && (
            <p>Finished {new Date(latest.finishedAt).toLocaleString()}</p>
          )}
          <ol className="space-y-2">
            {latest.steps.map((step) => (
              <li key={step.key}>
                <strong>{step.status}</strong> · {step.description}
                {step.sanitizedOutput && (
                  <pre className="whitespace-pre-wrap break-all text-xs">
                    {step.sanitizedOutput}
                  </pre>
                )}
              </li>
            ))}
          </ol>
          {running && (
            <Button
              variant="outline"
              disabled={cancel.isPending || latest.cancellationRequested}
              onClick={() => cancel.mutate()}
            >
              Request cancellation
            </Button>
          )}
          {!running && latest.status !== 'SUCCEEDED' && (
            <p>
              Retry by inspecting the server and approving a fresh plan. Remote
              state will be reconciled first.
            </p>
          )}
        </div>
      )}
      <details>
        <summary>Provisioning history ({history.data?.length ?? 0})</summary>
        <ul>
          {history.data?.map((run) => (
            <li key={run.id}>
              <Button variant="ghost" onClick={() => setSelectedRun(run.id)}>
                {new Date(run.startedAt).toLocaleString()} · {run.status}
              </Button>
              <span className="text-xs font-mono">{run.id}</span>
              {run.errorMessage && ` · ${run.errorMessage}`}
            </li>
          ))}
        </ul>
      </details>
      {selected && (
        <section
          className="panel space-y-3"
          aria-label="Historical provisioning run"
        >
          <h3>Run {selected.id}</h3>
          <p>
            {selected.status} · {selected.errorCode} {selected.errorMessage}
          </p>
          <p>
            {selected.approvedPlan
              ? `Approved plan ${selected.approvedPlan.id} · ${selected.approvedPlan.planHash}`
              : 'Original approved plan was not recorded by this backend version.'}
          </p>
          <ol>
            {selected.approvedPlan?.steps.map((step) => (
              <li key={step.key}>
                {step.description} · {step.status}
              </li>
            ))}
          </ol>
          <ol>
            {selected.steps.map((step) => (
              <li key={step.key}>
                {step.description} · {step.status} · {step.startedAt} ·{' '}
                {step.finishedAt}
              </li>
            ))}
          </ol>
          <ProvisioningLogViewer key={selected.id} run={selected} />
        </section>
      )}
    </section>
  );
}
