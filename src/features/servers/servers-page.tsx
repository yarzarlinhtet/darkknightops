import { useState, type FormEvent } from 'react';
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
  const mutation = useMutation({
    mutationFn: (value: ServerInput) =>
      server ? serversApi.update(server.id, value) : serversApi.create(value),
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
    mutation.mutate(input);
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
export function ServersPage() {
  const query = useQuery(serversQuery);
  const client = useQueryClient();
  const [editor, setEditor] = useState<{ server?: Server } | null>(null);
  const [pending, setPending] = useState(false);
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
        Metadata only · SSH connections and connection testing are not available
        in Milestone 1.
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
              <span className="status-badge">Connection unknown</span>
            </div>
            {server.notes && (
              <p className="mt-4 whitespace-pre-wrap break-words text-sm text-muted-foreground">
                {server.notes}
              </p>
            )}
            <p className="mt-4 text-xs text-muted-foreground">
              Updated {new Date(server.updatedAt).toLocaleString()}
            </p>
            <div className="mt-4 flex gap-2">
              <Button
                size="sm"
                variant="outline"
                aria-label={`Edit ${server.name}`}
                onClick={() => setEditor({ server })}
              >
                <Pencil />
                Edit
              </Button>
              <Button
                size="sm"
                variant="ghost"
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
              Save local metadata. No SSH connection will be made. Do not enter
              credentials or secrets in notes.
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
