import { sshMetadataKey } from './queries';
import { useEffect, useRef, useState } from 'react';
import { Link, useParams } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { KeySelector } from '@/features/keys/keys-page';
import { keysApi, serversApi, sshApi } from '@/lib/ipc';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ConnectionControls } from './ssh-controls';
import { usePreferences } from '@/store/preferences';

function CredentialEditor({
  id,
  lease,
  onChanged,
  disabled,
  onBusy,
}: {
  id: string;
  lease: string;
  onChanged: () => void;
  disabled: boolean;
  onBusy: (busy: boolean) => void;
}) {
  const client = useQueryClient();
  const metadata = useQuery({
    queryKey: sshMetadataKey(id),
    queryFn: () => sshApi.metadata(id),
    retry: false,
  });
  const [path, setPath] = useState<string | null>(null);
  const passphraseInput = useRef<HTMLInputElement>(null);
  const assigned = useQuery({
    queryKey: ['server-key', id],
    queryFn: () => keysApi.assigned(id),
  });
  const assignment = useMutation({
    mutationFn: (keyId: string | null) => keysApi.assign(id, keyId),
    onMutate: () => onBusy(true),
    onSuccess: async () => {
      onChanged();
      await client.invalidateQueries({ queryKey: ['server-key', id] });
      await client.invalidateQueries({ queryKey: ['ssh-keys'] });
    },
    onSettled: () => onBusy(false),
  });
  const [save, setSave] = useState(false);
  const select = useMutation({
    mutationFn: () => sshApi.selectKey(id),
    onSuccess: (p) => {
      if (p) setPath(p);
    },
  });
  const configure = useMutation({
    gcTime: 0,
    mutationFn: async (variables: { remove: boolean }) => {
      const { remove } = variables;
      let secret = passphraseInput.current?.value ?? '';
      if (passphraseInput.current) passphraseInput.current.value = '';
      try {
        const chosen = remove ? null : (path ?? metadata.data?.keyPath ?? null);
        await sshApi.configure(
          id,
          chosen,
          save && !remove ? secret : null,
          save && !remove,
        );
        if (!remove && secret && !save) await sshApi.unlock(id, lease, secret);
      } finally {
        secret = '';
      }
    },
    onMutate: () => onBusy(true),
    onSettled: () => onBusy(false),
    onSuccess: async () => {
      onChanged();
      await client.invalidateQueries({ queryKey: sshMetadataKey(id) });
    },
  });
  const unlock = useMutation({
    gcTime: 0,
    mutationFn: async () => {
      let secret = passphraseInput.current?.value ?? '';
      if (passphraseInput.current) passphraseInput.current.value = '';
      try {
        await sshApi.unlock(id, lease, secret);
      } finally {
        secret = '';
      }
    },
    onMutate: () => onBusy(true),
    onSettled: () => onBusy(false),
  });
  const pending =
    disabled || select.isPending || configure.isPending || unlock.isPending;
  return (
    <div className="space-y-3">
      <p className="text-sm">Authentication: SSH private key</p>
      <KeySelector
        value={assigned.data ?? null}
        disabled={pending || assignment.isPending}
        onChange={(key) => assignment.mutate(key || null)}
      />
      {assigned.data && (
        <Button
          variant="outline"
          disabled={pending || assignment.isPending}
          onClick={() => assignment.mutate(null)}
        >
          Use legacy credential
        </Button>
      )}
      {assignment.error && <p role="alert">{assignment.error.message}</p>}
      {assigned.data && (
        <p>
          The selected reusable key is used for authentication. Legacy settings
          below are retained for compatibility.
        </p>
      )}
      <p className="break-all font-mono text-sm">
        {path ?? metadata.data?.keyPath ?? 'No private key configured'}
      </p>
      <Button
        variant="outline"
        disabled={pending}
        onClick={() => select.mutate()}
      >
        Select private-key file
      </Button>
      <label className="block text-sm" htmlFor="passphrase">
        Key passphrase (optional)
      </label>
      <Input
        id="passphrase"
        type="password"
        autoComplete="off"
        ref={passphraseInput}
        disabled={pending}
      />
      <label className="flex gap-2 text-sm">
        <input
          type="checkbox"
          checked={save}
          onChange={(e) => setSave(e.target.checked)}
          disabled={pending}
        />
        Save passphrase in macOS Keychain
      </label>
      <p className="text-xs text-muted-foreground">
        Unsaved passphrases stay in Rust memory until this detail session closes
        or expires. Saved passphrase:{' '}
        {metadata.data?.passphraseSaved ? 'Yes' : 'No'}.
      </p>
      <div className="flex flex-wrap gap-2">
        <Button
          disabled={pending}
          onClick={() => {
            configure.mutate({ remove: false });
          }}
        >
          Save SSH configuration
        </Button>
        <Button
          variant="outline"
          disabled={pending}
          onClick={() => {
            unlock.mutate();
          }}
        >
          Unlock for this session
        </Button>
        <Button
          variant="destructive"
          disabled={pending || !metadata.data?.keyPath}
          onClick={() => {
            if (
              window.confirm(
                'Remove the local SSH credential reference and saved passphrase?',
              )
            ) {
              if (passphraseInput.current) passphraseInput.current.value = '';
              configure.mutate({ remove: true });
            }
          }}
        >
          Remove credential
        </Button>
      </div>
      {(metadata.error || select.error || configure.error || unlock.error) && (
        <p role="alert" className="text-destructive">
          {
            (metadata.error || select.error || configure.error || unlock.error)
              ?.message
          }
        </p>
      )}
      {configure.isSuccess && <p role="status">SSH configuration saved.</p>}
      {unlock.isSuccess && (
        <p role="status">Passphrase available for the next connection test.</p>
      )}
    </div>
  );
}
const bytes = (value: number) => `${(value / 1024 ** 3).toFixed(1)} GiB`;
const percent = (value: number | null | undefined) =>
  value == null ? 'Unavailable' : `${value.toFixed(1)}%`;
export function ServerDetailPage() {
  const { id = '' } = useParams();
  return <ServerDetail key={id} id={id} />;
}
let detailLifecycle: Promise<unknown> = Promise.resolve();
function ServerDetail({ id }: { id: string }) {
  const client = useQueryClient();
  const server = useQuery({
    queryKey: ['server', id],
    queryFn: () => serversApi.get(id),
    retry: false,
  });
  const metadata = useQuery({
    queryKey: sshMetadataKey(id),
    queryFn: () => sshApi.metadata(id),
    retry: false,
  });
  const [lease, setLease] = useState<string>();
  const leaseRef = useRef<string | undefined>(undefined);
  const alive = useRef(true);
  const [sessionError, setSessionError] = useState<string>();
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [clock, setClock] = useState(() => Date.now());
  const [visible, setVisible] = useState(!document.hidden);
  const { metricsInterval, setMetricsInterval } = usePreferences();
  useEffect(() => {
    alive.current = true;
    let closed = false;
    detailLifecycle = detailLifecycle
      .then(async () => {
        if (closed) return;
        const value = await sshApi.open(id);
        if (closed) await sshApi.close(id, value);
        else {
          leaseRef.current = value;
          setLease(value);
        }
      })
      .catch((e: Error) => {
        if (!closed) setSessionError(e.message);
      });
    const visibility = () => setVisible(!document.hidden);
    document.addEventListener('visibilitychange', visibility);
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => {
      closed = true;
      alive.current = false;
      window.clearInterval(timer);
      document.removeEventListener('visibilitychange', visibility);
      const currentLease = leaseRef.current;
      leaseRef.current = undefined;
      if (currentLease)
        detailLifecycle = detailLifecycle
          .then(() => sshApi.close(id, currentLease))
          .catch(() => {});
      void client.cancelQueries({ queryKey: ['metrics', id] });
      client.removeQueries({ queryKey: ['metrics', id] });
    };
  }, [id, client]);
  const refresh = useMutation({
    mutationFn: async () => {
      if (!lease)
        throw new Error('Reopen this server to create a detail session.');
      await client.cancelQueries({ queryKey: ['metrics', id] });
      const info = await sshApi.info(id, lease);
      const docker = await sshApi.docker(id, lease);
      return { info, docker };
    },
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: sshMetadataKey(id) });
    },
    onError: () => setConnected(false),
  });
  const metrics = useQuery({
    queryKey: ['metrics', id, lease],
    queryFn: async ({ signal }) => {
      if (!lease) throw new Error('Open a detail session first.');
      const cancel = () => {
        void sshApi.cancel(id).catch(() => {});
      };
      signal.addEventListener('abort', cancel, { once: true });
      try {
        return await sshApi.metrics(id, lease);
      } finally {
        signal.removeEventListener('abort', cancel);
      }
    },
    enabled: !!lease && connected && visible && !busy && !refresh.isPending,
    refetchInterval: (query) =>
      connected && visible && !busy && !refresh.isPending && !query.state.error
        ? metricsInterval * 1000
        : false,
    refetchOnWindowFocus: false,
    retry: false,
    gcTime: 0,
  });
  const data = metadata.data;
  const info = data?.info;
  const docker = data?.docker;
  const live = metrics.data;
  const stale =
    !!metrics.error ||
    !connected ||
    (live
      ? clock - Date.parse(live.sampledAt) > metricsInterval * 2000
      : false);
  if (server.isPending)
    return (
      <div role="status" className="panel animate-pulse">
        Loading server details…
      </div>
    );
  if (server.error)
    return (
      <div role="alert" className="panel">
        {server.error.message}
        <Link to="/servers">Back to servers</Link>
      </div>
    );
  return (
    <div className="space-y-5">
      <Link to="/servers" className="text-sm text-primary">
        ← Servers
      </Link>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Read-only discovery</p>
          <h1>{server.data.name}</h1>
          <p className="mt-2 break-all">
            {server.data.username}@{server.data.host}:{server.data.port}
          </p>
        </div>
        <span role="status" className="status-badge">
          {busy
            ? 'Operation in progress'
            : connected && !metrics.error
              ? 'SSH session verified'
              : 'Disconnected / not currently verified'}
        </span>
      </div>
      {(sessionError || metadata.error) && (
        <p role="alert" className="text-destructive">
          {sessionError || metadata.error?.message}
        </p>
      )}
      <section className="panel space-y-4">
        <h2>Actions</h2>
        <ConnectionControls
          id={id}
          lease={lease}
          prepare={async () => {
            const current = await sshApi.open(id);
            if (!alive.current) {
              await sshApi.close(id, current);
              throw new Error('Detail session closed.');
            }
            leaseRef.current = current;
            setLease(current);
            return current;
          }}
          disabled={!lease || busy || refresh.isPending || metrics.isFetching}
          onBusy={setBusy}
          onResult={(result) => {
            if (!alive.current) return;
            setConnected(result.status === 'CONNECTED');
            if (result.status === 'CONNECTED') refresh.mutate();
          }}
        />
        <div className="flex flex-wrap gap-2">
          <Button
            variant="outline"
            disabled={
              !connected || busy || refresh.isPending || metrics.isFetching
            }
            onClick={() => refresh.mutate()}
          >
            Refresh Server Information
          </Button>
          <Button
            variant="outline"
            disabled={
              !connected || busy || refresh.isPending || metrics.isFetching
            }
            onClick={() => void metrics.refetch()}
          >
            Refresh Metrics
          </Button>
          {(busy || metrics.isFetching) && (
            <Button
              variant="ghost"
              onClick={() => {
                void sshApi
                  .cancel(id)
                  .catch((e: Error) => setSessionError(e.message));
              }}
            >
              Cancel operation
            </Button>
          )}
        </div>
        <label className="flex items-center gap-3 text-sm">
          Metrics refresh
          <select
            value={metricsInterval}
            onChange={(e) => setMetricsInterval(Number(e.target.value))}
            className="rounded border bg-background p-2"
          >
            {[5, 10, 30, 60].map((v) => (
              <option key={v} value={v}>
                {v} seconds
              </option>
            ))}
          </select>
        </label>
        {(refresh.error || metrics.error) && (
          <p role="alert" className="text-destructive">
            {(refresh.error || metrics.error)?.message} Re-test the connection
            to reconnect.
          </p>
        )}
      </section>
      <section className="panel space-y-3">
        <h2>Overview</h2>
        {metrics.isFetching && <p role="status">Refreshing metrics…</p>}
        {live && (
          <p className="text-xs text-muted-foreground">
            {stale ? 'Stale data' : 'Last sampled'}:{' '}
            {new Date(live.sampledAt).toLocaleString()}
          </p>
        )}
        <dl className="grid gap-4 sm:grid-cols-3">
          <div>
            <dt>CPU</dt>
            <dd>{percent(live?.cpuPercent)}</dd>
          </div>
          <div>
            <dt>Memory</dt>
            <dd>{percent(live?.memoryPercent)}</dd>
          </div>
          <div>
            <dt>Root disk</dt>
            <dd>{percent(live?.diskPercent)}</dd>
          </div>
          <div>
            <dt>Hostname</dt>
            <dd>{info?.hostname ?? 'Not discovered'}</dd>
          </div>
          <div>
            <dt>Operating system</dt>
            <dd>{info?.distribution ?? 'Not discovered'}</dd>
          </div>
          <div>
            <dt>Architecture / kernel</dt>
            <dd>
              {info
                ? `${info.architecture} / ${info.kernel}`
                : 'Not discovered'}
            </dd>
          </div>
          <div>
            <dt>Uptime</dt>
            <dd>
              {live || info
                ? `${Math.floor((live?.uptimeSeconds ?? info?.uptimeSeconds ?? 0) / 60)} minutes`
                : 'Unavailable'}
            </dd>
          </div>
          <div>
            <dt>Logical CPUs</dt>
            <dd>{info?.logicalCpus ?? 'Unavailable'}</dd>
          </div>
          <div>
            <dt>Load average</dt>
            <dd>{info?.loadAverage.join(' / ') ?? 'Unavailable'}</dd>
          </div>
          <div>
            <dt>Memory available / total</dt>
            <dd>
              {info
                ? `${bytes(info.memoryAvailableBytes)} / ${bytes(info.memoryTotalBytes)}`
                : 'Unavailable'}
            </dd>
          </div>
          <div>
            <dt>Root disk available / total</dt>
            <dd>
              {info
                ? `${bytes(info.diskAvailableBytes)} / ${bytes(info.diskTotalBytes)}`
                : 'Unavailable'}
            </dd>
          </div>
        </dl>
        <p className="text-xs text-muted-foreground">
          {info
            ? `Snapshot: ${new Date(info.discoveredAt).toLocaleString()}. Provisioning platform support: ${info.provisioningSupported ? 'Ubuntu 24.04 supported in a future milestone' : 'not supported'}.`
            : 'Test the connection, then refresh server information.'}
        </p>
      </section>
      <section className="panel space-y-3">
        <h2>Docker</h2>
        <p>
          CLI installed:{' '}
          {docker ? (docker.cliInstalled ? 'Yes' : 'No') : 'Unknown'}
        </p>
        <p>Daemon access: {docker?.daemonAccess ?? 'Unknown'}</p>
        <p>
          Current sampled daemon availability:{' '}
          {live
            ? live.dockerAvailable
              ? 'Available'
              : 'Unavailable'
            : 'Unknown'}
        </p>
        <p>
          Client / Engine: {docker?.clientVersion ?? 'Unknown'} /{' '}
          {docker?.serverVersion ?? 'Unknown'}
        </p>
        <p>
          Compose v2:{' '}
          {docker ? (docker.composeVersion ?? 'Missing') : 'Unknown'}
        </p>
        <p>
          Access without sudo:{' '}
          {docker ? (docker.accessWithoutSudo ? 'Yes' : 'No') : 'Unknown'}
        </p>
        {docker?.versionDetectionFailed && (
          <p role="alert">Docker version detection failed.</p>
        )}
        <p className="text-xs text-muted-foreground">
          {docker
            ? `Snapshot: ${new Date(docker.discoveredAt).toLocaleString()}`
            : 'Refresh server information to detect Docker.'}{' '}
          Docker daemon access is root-equivalent.
        </p>
      </section>
      <section className="panel space-y-4">
        <h2>SSH</h2>
        <p className="break-all text-sm">
          Trusted key: {data?.trustedAlgorithm ?? ''}{' '}
          {data?.trustedFingerprint ?? 'No trusted identity'}
        </p>
        <p className="text-sm">
          Last connection test (historical):{' '}
          {data?.lastTest
            ? `${data.lastTest.status} · ${new Date(data.lastTest.testedAt).toLocaleString()}`
            : 'Never'}
        </p>
        <p className="text-sm">
          Last successful test:{' '}
          {data?.lastSuccessAt
            ? new Date(data.lastSuccessAt).toLocaleString()
            : 'Never'}
        </p>
        {lease && (
          <CredentialEditor
            id={id}
            lease={lease}
            disabled={busy || refresh.isPending || metrics.isFetching}
            onBusy={setBusy}
            onChanged={() => setConnected(false)}
          />
        )}
      </section>
    </div>
  );
}
