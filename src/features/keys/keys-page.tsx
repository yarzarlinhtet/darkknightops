import { useRef, useState, type FormEvent } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { keysApi, sshApi } from '@/lib/ipc';
import type { SshKey } from '@/lib/bindings';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
const keysQuery = { queryKey: ['ssh-keys'], queryFn: keysApi.list };

export function KeyDialog({
  mode,
  onClose,
  onCreated,
}: {
  mode: 'generate' | 'import';
  onClose: () => void;
  onCreated: (key: SshKey) => void;
}) {
  const form = useRef<HTMLFormElement>(null);
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [path, setPath] = useState('');
  async function submit(e: FormEvent) {
    e.preventDefault();
    if (!form.current) return;
    const data = new FormData(form.current);
    let secret = String(data.get('passphrase') ?? '');
    const confirm = String(data.get('confirm') ?? '');
    if (mode === 'generate' && secret !== confirm) {
      setError('Passphrases do not match.');
      return;
    }
    const name = String(data.get('name') ?? '');
    const save = data.get('save') === 'on';
    setBusy(true);
    setError('');
    form.current
      .querySelectorAll<HTMLInputElement>('input[type="password"]')
      .forEach((i) => {
        i.value = '';
      });
    data.delete('passphrase');
    data.delete('confirm');
    try {
      const key =
        mode === 'generate'
          ? await keysApi.generate(
              name,
              data.get('algorithm') === 'RSA',
              secret || null,
              save,
            )
          : await keysApi.import(
              name,
              path,
              data.get('managed') === 'on',
              secret || null,
              save,
            );
      await client.invalidateQueries({ queryKey: keysQuery.queryKey });
      onCreated(key);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Key operation failed.');
    } finally {
      secret = '';
      setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent>
        <DialogTitle>
          {mode === 'generate' ? 'Generate SSH key' : 'Import SSH key'}
        </DialogTitle>
        <DialogDescription>
          Private keys remain on this Mac. Public keys can be installed on
          multiple servers.
        </DialogDescription>
        <form ref={form} onSubmit={(e) => void submit(e)} className="space-y-4">
          <fieldset disabled={busy} className="space-y-4">
            <label className="block">
              Key name
              <Input name="name" required maxLength={128} />
            </label>
            {mode === 'generate' ? (
              <label className="block">
                Algorithm
                <select
                  name="algorithm"
                  className="block w-full border rounded p-2 bg-background"
                >
                  <option value="ED25519">Ed25519 (recommended)</option>
                  <option value="RSA">RSA 4096 (compatibility)</option>
                </select>
              </label>
            ) : (
              <>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => {
                    void sshApi
                      .selectKey()
                      .then((p) => {
                        if (p) setPath(p);
                      })
                      .catch((e) => setError(e.message));
                  }}
                >
                  Select private-key file
                </Button>
                <p className="break-all text-sm">
                  {path || 'No file selected'}
                </p>
                <label className="flex gap-2">
                  <input type="checkbox" name="managed" />
                  Copy into managed storage
                </label>
              </>
            )}
            <label className="block">
              Passphrase (optional)
              <Input name="passphrase" type="password" autoComplete="off" />
            </label>
            {mode === 'generate' && (
              <label className="block">
                Confirm passphrase
                <Input name="confirm" type="password" autoComplete="off" />
              </label>
            )}
            <label className="flex gap-2">
              <input name="save" type="checkbox" />
              Save passphrase in macOS Keychain
            </label>
            <Button disabled={mode === 'import' && !path} type="submit">
              {busy
                ? 'Working…'
                : mode === 'generate'
                  ? 'Generate key'
                  : 'Import key'}
            </Button>
            <Button type="button" variant="outline" onClick={onClose}>
              Cancel
            </Button>
          </fieldset>
          {error && (
            <p role="alert" className="text-destructive">
              {error}
            </p>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}
export function KeySelector({
  value,
  onChange,
  disabled,
}: {
  value: string | null;
  onChange: (id: string) => void;
  disabled?: boolean;
}) {
  const query = useQuery(keysQuery);
  const [mode, setMode] = useState<'generate' | 'import' | null>(null);
  const selected = query.data?.find((k) => k.id === value);
  return (
    <div className="space-y-2">
      <label className="block">
        SSH Key
        <select
          aria-label="SSH Key"
          className="block w-full rounded border bg-background p-2"
          disabled={disabled || query.isPending}
          value={value ?? ''}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Keep existing / configure later</option>
          {query.data?.map((k) => (
            <option key={k.id} value={k.id}>
              {k.name} · {k.algorithm} · {k.fingerprintSha256.slice(0, 23)}… ·{' '}
              {k.storageType}
            </option>
          ))}
        </select>
      </label>
      {selected && (
        <p className="break-all text-xs">{selected.fingerprintSha256}</p>
      )}
      {query.isPending && <p role="status">Loading keys…</p>}
      {query.error && <p role="alert">{query.error.message}</p>}
      {query.data?.length === 0 && <p>No SSH keys yet.</p>}
      <div className="flex gap-2">
        <Button
          type="button"
          disabled={disabled}
          variant="outline"
          onClick={() => setMode('generate')}
        >
          Generate New Key
        </Button>
        <Button
          type="button"
          disabled={disabled}
          variant="outline"
          onClick={() => setMode('import')}
        >
          Import Existing Key
        </Button>
      </div>
      {mode && (
        <KeyDialog
          mode={mode}
          onClose={() => setMode(null)}
          onCreated={(k) => {
            onChange(k.id);
            setMode(null);
          }}
        />
      )}
    </div>
  );
}
function KeyDetails({ item, onClose }: { item: SshKey; onClose: () => void }) {
  const client = useQueryClient();
  const servers = useQuery({
    queryKey: ['ssh-key-servers', item.id],
    queryFn: () => keysApi.servers(item.id),
  });
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [deleteFile, setDeleteFile] = useState(false);
  const [name, setName] = useState(item.name);
  const [status, setStatus] = useState('');
  async function act(fn: () => Promise<unknown>) {
    setBusy(true);
    setError('');
    try {
      await fn();
      await client.invalidateQueries({ queryKey: keysQuery.queryKey });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Operation failed.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent>
        <DialogTitle>{item.name}</DialogTitle>
        <DialogDescription>
          {item.algorithm} · {item.storageType}
        </DialogDescription>
        <p className="break-all font-mono text-sm">{item.fingerprintSha256}</p>
        <textarea
          aria-label="Public key"
          readOnly
          value={item.publicKey}
          className="w-full h-28 border rounded p-2 bg-background font-mono text-xs"
        />
        <div className="flex gap-2">
          <Button
            disabled={busy}
            onClick={() =>
              void act(async () => {
                await navigator.clipboard.writeText(item.publicKey);
                setStatus('Public key copied.');
              })
            }
          >
            Copy public key
          </Button>
          <Button
            disabled={busy}
            variant="outline"
            onClick={() =>
              void act(async () => {
                if (await keysApi.export(item.id))
                  setStatus('Public key exported.');
              })
            }
          >
            Export .pub
          </Button>
        </div>
        <label>
          Key name
          <Input
            value={name}
            maxLength={128}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <Button
          disabled={busy}
          variant="outline"
          onClick={() => void act(() => keysApi.rename(item.id, name))}
        >
          Rename
        </Button>
        <p>Associated servers</p>
        {servers.isPending ? (
          <p>Loading…</p>
        ) : servers.error ? (
          <p role="alert">{servers.error.message}</p>
        ) : servers.data?.length ? (
          servers.data.map((s) => (
            <Link
              className="block text-primary underline"
              key={s.id}
              to={`/servers/${s.id}`}
            >
              {s.name}
            </Link>
          ))
        ) : (
          <p>No associated servers.</p>
        )}
        {item.serverCount > 0 ? (
          <p>Unassign this key from all servers before deleting it.</p>
        ) : !confirm ? (
          <Button
            disabled={busy}
            variant="destructive"
            onClick={() => setConfirm(true)}
          >
            Delete key…
          </Button>
        ) : (
          <div className="space-y-2">
            <p>Remove this key from RemoteDock?</p>
            {item.storageType === 'MANAGED' ? (
              <label className="flex gap-2">
                <input
                  type="checkbox"
                  checked={deleteFile}
                  onChange={(e) => setDeleteFile(e.target.checked)}
                />
                Also permanently delete the managed private-key file
              </label>
            ) : (
              <p>The original private-key file will be retained.</p>
            )}
            <Button
              variant="destructive"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await keysApi.delete(item.id, deleteFile);
                  onClose();
                })
              }
            >
              Confirm removal
            </Button>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => setConfirm(false)}
            >
              Cancel
            </Button>
          </div>
        )}
        {status && <p role="status">{status}</p>}
        {error && (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}
export function SshKeysPage() {
  const query = useQuery(keysQuery);
  const [mode, setMode] = useState<'generate' | 'import' | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const item = query.data?.find((k) => k.id === selected);
  return (
    <>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Workspace</p>
          <h1>SSH Keys</h1>
          <p>Reusable authentication keys for your servers.</p>
        </div>
        <div className="flex gap-2">
          <Button onClick={() => setMode('generate')}>Generate key</Button>
          <Button variant="outline" onClick={() => setMode('import')}>
            Import key
          </Button>
        </div>
      </div>
      <section className="panel overflow-x-auto">
        {query.isPending ? (
          <p role="status">Loading SSH keys…</p>
        ) : query.error ? (
          <p role="alert">{query.error.message}</p>
        ) : !query.data.length ? (
          <p>No SSH keys yet. Generate or import a key to get started.</p>
        ) : (
          <table className="w-full text-left text-sm">
            <thead>
              <tr>
                {[
                  'Name',
                  'Algorithm',
                  'Fingerprint',
                  'Servers',
                  'Storage',
                  'Created',
                ].map((h) => (
                  <th className="p-2" key={h}>
                    {h}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {query.data.map((k) => (
                <tr key={k.id}>
                  <td className="p-2">
                    <Button variant="ghost" onClick={() => setSelected(k.id)}>
                      {k.name}
                    </Button>
                  </td>
                  <td>{k.algorithm}</td>
                  <td className="font-mono" title={k.fingerprintSha256}>
                    {k.fingerprintSha256.slice(0, 23)}…
                  </td>
                  <td>{k.serverCount}</td>
                  <td>{k.storageType}</td>
                  <td>{new Date(k.createdAt).toLocaleDateString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      {mode && (
        <KeyDialog
          mode={mode}
          onClose={() => setMode(null)}
          onCreated={(k) => {
            setMode(null);
            setSelected(k.id);
          }}
        />
      )}
      {item && (
        <KeyDetails
          key={item.id}
          item={item}
          onClose={() => setSelected(null)}
        />
      )}
    </>
  );
}
