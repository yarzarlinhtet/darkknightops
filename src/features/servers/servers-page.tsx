import { sshMetadataKey } from './queries';
import { Link } from 'react-router-dom';
import { ConnectionControls } from './ssh-controls';
import { KeySelector } from '@/features/keys/keys-page';
import { keysApi, sshApi } from '@/lib/ipc';
import { useRef, useState, type FormEvent } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Plus, Pencil, Trash2, Server as ServerIcon } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogCancel,
} from '@/components/ui/alert-dialog';
import { IpcError, serversApi, type Server, type ServerInput } from '@/lib/ipc';
import { serversQuery } from './queries';
import { validateServer, type FieldErrors } from './validation';

function ServerEditor({
  server,
  onSaved,
  onPending,
}: {
  server?: Server;
  onSaved: () => void;
  onPending: (pending: boolean) => void;
}) {
  const client = useQueryClient();
  const [input, setInput] = useState<ServerInput>(
    server
      ? {
          name: server.name,
          host: server.host,
          username: server.username,
          port: server.port,
          notes: server.notes,
        }
      : { name: '', host: '', username: '', port: 22, notes: null },
  );
  const [errors, setErrors] = useState<FieldErrors>({});
  const [keyPath, setKeyPath] = useState<string | null>(null);
  const passphraseInput = useRef<HTMLInputElement>(null);
  const assignedKey = useQuery({
    queryKey: ['server-key', server?.id],
    queryFn: () => keysApi.assigned(server!.id),
    enabled: !!server,
  });
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [savePassphrase, setSavePassphrase] = useState(false);
  const [createdId, setCreatedId] = useState<string>();
  const selectKey = useMutation({
    mutationFn: () => sshApi.selectKey(server?.id),
    onSuccess: (path) => {
      if (path) setKeyPath(path);
    },
  });
  const mutation = useMutation({
    gcTime: 0,
    mutationFn: async (variables: { value: ServerInput }) => {
      let secret = passphraseInput.current?.value ?? '';
      if (passphraseInput.current) passphraseInput.current.value = '';
      try {
        const id = server?.id ?? createdId;
        const saved = id
          ? selectedKey
            ? await serversApi.update(id, variables.value, selectedKey)
            : await serversApi.update(id, variables.value)
          : selectedKey
            ? await serversApi.create(variables.value, selectedKey)
            : await serversApi.create(variables.value);
        setCreatedId(saved.id);
        await client.invalidateQueries({ queryKey: serversQuery.queryKey });
        if (keyPath)
          await sshApi.configure(
            saved.id,
            keyPath,
            savePassphrase ? secret : null,
            savePassphrase,
          );
        return saved;
      } finally {
        secret = '';
      }
    },
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: serversQuery.queryKey });
      onSaved();
    },
    onError: (error) => {
      if (error instanceof IpcError && error.field)
        setErrors({ [error.field]: error.message });
    },
    onSettled: () => onPending(false),
  });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const issues = validateServer(input);
    setErrors(issues);
    if (Object.keys(issues).length) return;
    onPending(true);
    mutation.mutate({ value: input });
  };
  return (
    <form onSubmit={submit} noValidate className="mt-6 space-y-4">
      <fieldset disabled={mutation.isPending} className="space-y-4">
        {(['name', 'host', 'username', 'port'] as const).map((field) => (
          <div key={field}>
            <label className="mb-1.5 block text-sm font-medium" htmlFor={field}>
              {
                {
                  name: 'Display name',
                  host: 'Hostname or IP address',
                  username: 'SSH username',
                  port: 'SSH port',
                }[field]
              }
            </label>
            <Input
              id={field}
              name={field}
              autoComplete="off"
              type={field === 'port' ? 'number' : 'text'}
              value={input[field]}
              min={field === 'port' ? 1 : undefined}
              max={field === 'port' ? 65535 : undefined}
              aria-invalid={!!errors[field]}
              aria-describedby={errors[field] ? `${field}-error` : undefined}
              onChange={(e) => {
                setInput({
                  ...input,
                  [field]:
                    field === 'port' ? Number(e.target.value) : e.target.value,
                });
                setErrors({ ...errors, [field]: undefined });
              }}
            />
            {errors[field] && (
              <p
                id={`${field}-error`}
                className="mt-1 text-sm text-destructive"
              >
                {errors[field]}
              </p>
            )}
          </div>
        ))}
        <div className="space-y-3">
          <label className="block text-sm">
            Authentication method
            <select
              className="mt-1 w-full rounded-md border bg-background p-2"
              value="private-key"
              disabled
            >
              <option value="private-key">SSH private key</option>
            </select>
          </label>
          <KeySelector
            value={selectedKey ?? assignedKey.data ?? null}
            onChange={setSelectedKey}
            disabled={mutation.isPending}
          />
          <Button
            type="button"
            variant="outline"
            disabled={selectKey.isPending || !!selectedKey}
            onClick={() => selectKey.mutate()}
          >
            Select SSH private-key file
          </Button>
          <p className="break-all text-xs text-muted-foreground">
            {keyPath ??
              (server
                ? 'Keep existing SSH credential; manage it in server details.'
                : 'Optional now; configure in server details before connecting.')}
          </p>
          {keyPath && (
            <>
              <label className="block text-sm" htmlFor="editor-passphrase">
                Private-key passphrase
              </label>
              <Input
                id="editor-passphrase"
                type="password"
                autoComplete="off"
                ref={passphraseInput}
              />
              <label className="flex gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={savePassphrase}
                  onChange={(e) => setSavePassphrase(e.target.checked)}
                />
                Save passphrase in macOS Keychain
              </label>
              <p className="text-xs text-muted-foreground">
                If you do not save the passphrase, enter it in server details
                when connecting.
              </p>
            </>
          )}
          {selectKey.error && (
            <p role="alert" className="text-destructive">
              {selectKey.error.message}
            </p>
          )}
        </div>
        <div>
          <label htmlFor="notes" className="mb-1.5 block text-sm font-medium">
            Notes (optional)
          </label>
          <textarea
            id="notes"
            className="min-h-20 w-full rounded-md border bg-background p-3 text-sm"
            value={input.notes ?? ''}
            aria-invalid={!!errors.notes}
            aria-describedby={errors.notes ? 'notes-error' : undefined}
            onChange={(e) => {
              setInput({ ...input, notes: e.target.value || null });
              setErrors({ ...errors, notes: undefined });
            }}
          />
          {errors.notes && (
            <p id="notes-error" className="text-sm text-destructive">
              {errors.notes}
            </p>
          )}
        </div>
      </fieldset>
      {mutation.error && (
        <p role="alert" className="text-sm text-destructive">
          {createdId && !server
            ? 'Server metadata saved. Complete SSH setup or open its details. '
            : ''}
          {mutation.error.message}
        </p>
      )}
      <div className="flex justify-end">
        <Button type="submit" disabled={mutation.isPending}>
          {mutation.isPending
            ? 'Saving…'
            : server
              ? 'Save changes'
              : 'Add server'}
        </Button>
      </div>
    </form>
  );
}
function ServerConnectionSummary({
  id,
  onBusy,
}: {
  id: string;
  onBusy: (busy: boolean) => void;
}) {
  const query = useQuery({
    queryKey: sshMetadataKey(id),
    queryFn: () => sshApi.metadata(id),
    retry: false,
  });
  return (
    <div className="mt-3 space-y-2 text-xs text-muted-foreground">
      <p>
        Last result: {query.data?.lastTest?.status ?? 'Never tested'} · Docker:{' '}
        {query.data?.docker?.daemonAccess ?? 'Unknown'}
      </p>
      <p>
        Last checked:{' '}
        {query.data?.lastTest
          ? new Date(query.data.lastTest.testedAt).toLocaleString()
          : 'Never'}
      </p>
      {query.error && <p role="alert">{query.error.message}</p>}
      <ConnectionControls id={id} onBusy={onBusy} />
    </div>
  );
}
export function ServersPage() {
  const query = useQuery(serversQuery);
  const client = useQueryClient();
  const [editor, setEditor] = useState<{ server?: Server } | null>(null);
  const [pending, setPending] = useState(false);
  const [busyServers, setBusyServers] = useState<Set<string>>(new Set());
  const [deleting, setDeleting] = useState<Server | null>(null);
  const remove = useMutation({
    mutationFn: serversApi.delete,
    onSuccess: async () => {
      await client.invalidateQueries({ queryKey: serversQuery.queryKey });
      setDeleting(null);
    },
  });
  return (
    <>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Infrastructure</p>
          <h1>Servers</h1>
          <p className="text-muted-foreground">
            Your saved server inventory, stored on this Mac.
          </p>
        </div>
        <Button
          disabled={query.isError || query.isPending}
          onClick={() => setEditor({})}
        >
          <Plus />
          Add server
        </Button>
      </div>
      <div className="notice">
        SSH discovery is read-only. Open server details to configure a private
        key and verify the host identity.
      </div>
      {query.isPending && <p role="status">Loading local servers…</p>}
      {query.error && (
        <div role="alert" className="panel space-y-3">
          <h2>Cannot load servers</h2>
          <p>{query.error.message}</p>
          <Button variant="outline" onClick={() => void query.refetch()}>
            Retry
          </Button>
        </div>
      )}
      {query.isSuccess && query.data.length === 0 && (
        <div className="panel empty-state">
          <ServerIcon className="mx-auto mb-4 size-9 text-muted-foreground" />
          <h2>No servers yet</h2>
          <p>Save a hostname and SSH username to start your local inventory.</p>
          <Button onClick={() => setEditor({})}>Add your first server</Button>
        </div>
      )}
      <div className="grid gap-4 xl:grid-cols-2">
        {query.data?.map((server) => (
          <article className="panel" key={server.id}>
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h2 className="break-words">{server.name}</h2>
                <p className="mt-1 break-all font-mono text-sm text-muted-foreground">
                  {server.username}@{server.host}:{server.port}
                </p>
              </div>
              <span className="status-badge">Live status unknown</span>
            </div>
            {server.notes && (
              <p className="mt-4 whitespace-pre-wrap break-words text-sm text-muted-foreground">
                {server.notes}
              </p>
            )}
            <p className="mt-4 text-xs text-muted-foreground">
              Updated {new Date(server.updatedAt).toLocaleString()}
            </p>
            <ServerConnectionSummary
              id={server.id}
              onBusy={(busy) =>
                setBusyServers((current) => {
                  const next = new Set(current);
                  if (busy) next.add(server.id);
                  else next.delete(server.id);
                  return next;
                })
              }
            />
            <div className="mt-4 flex flex-wrap gap-2">
              <Button asChild size="sm" variant="outline">
                <Link to={`/servers/${server.id}`}>Open details / SSH</Link>
              </Button>
              <Button
                size="sm"
                variant="outline"
                disabled={busyServers.has(server.id)}
                aria-label={`Edit ${server.name}`}
                onClick={() => setEditor({ server })}
              >
                <Pencil />
                Edit
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busyServers.has(server.id)}
                aria-label={`Delete ${server.name}`}
                onClick={() => {
                  remove.reset();
                  setDeleting(server);
                }}
              >
                <Trash2 />
                Delete
              </Button>
            </div>
          </article>
        ))}
      </div>
      <Dialog
        open={editor !== null}
        onOpenChange={(open) => {
          if (!open && !pending) setEditor(null);
        }}
      >
        {editor && (
          <DialogContent
            onEscapeKeyDown={(e) => {
              if (pending) e.preventDefault();
            }}
            onInteractOutside={(e) => {
              if (pending) e.preventDefault();
            }}
          >
            <DialogTitle className="text-xl font-semibold">
              {editor.server ? 'Edit server' : 'Add server'}
            </DialogTitle>
            <DialogDescription className="mt-2 text-sm text-muted-foreground">
              Save server details, then open the server to configure its SSH
              key. Do not enter secrets in notes.
            </DialogDescription>
            <ServerEditor
              server={editor.server}
              onPending={setPending}
              onSaved={() => setEditor(null)}
            />
          </DialogContent>
        )}
      </Dialog>
      <AlertDialog
        open={deleting !== null}
        onOpenChange={(open) => {
          if (!open && !remove.isPending) setDeleting(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogTitle className="text-xl font-semibold">
            Delete server?
          </AlertDialogTitle>
          <AlertDialogDescription className="mt-3 text-sm text-muted-foreground">
            Remove “{deleting?.name}” from this Mac? This deletes local metadata
            only and cannot be undone.
          </AlertDialogDescription>
          {remove.error && (
            <p role="alert" className="mt-3 text-sm text-destructive">
              {remove.error.message}
            </p>
          )}
          <div className="mt-6 flex justify-end gap-2">
            <AlertDialogCancel asChild>
              <Button variant="outline" disabled={remove.isPending}>
                Cancel
              </Button>
            </AlertDialogCancel>
            <Button
              variant="destructive"
              disabled={remove.isPending}
              onClick={() => deleting && remove.mutate(deleting.id)}
            >
              {remove.isPending ? 'Deleting…' : 'Delete server'}
            </Button>
          </div>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
