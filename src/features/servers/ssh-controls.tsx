import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { sshApi } from '@/lib/ipc';
import type { ConnectionTestResult, HostKeyObservation } from '@/lib/bindings';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog';

import { sshMetadataKey } from './queries';
export function ConnectionControls({
  id,
  lease,
  disabled,
  onResult,
  onBusy,
  prepare,
}: {
  id: string;
  lease?: string;
  disabled?: boolean;
  prepare?: () => Promise<string>;
  onResult?: (result: ConnectionTestResult) => void;
  onBusy?: (busy: boolean) => void;
}) {
  const client = useQueryClient();
  const [observation, setObservation] = useState<HostKeyObservation | null>(
    null,
  );
  const [review, setReview] = useState(false);
  const [verified, setVerified] = useState(false);
  const test = useMutation({
    mutationFn: async () => sshApi.test(id, prepare ? await prepare() : lease),
    onMutate: () => onBusy?.(true),
    onSuccess: (result) => {
      setObservation(result.observation);
      setReview(false);
      setVerified(false);
      onResult?.(result);
      void client.invalidateQueries({ queryKey: sshMetadataKey(id) });
    },
    onSettled: () => onBusy?.(false),
  });
  const trust = useMutation({
    mutationFn: () => {
      if (!observation) throw new Error('Test the connection again.');
      return sshApi.trust(
        id,
        observation.id,
        observation.previousFingerprint !== null,
      );
    },
    onMutate: () => onBusy?.(true),
    onSuccess: async () => {
      setObservation(null);
      await client.invalidateQueries({ queryKey: sshMetadataKey(id) });
      test.mutate();
    },
    onSettled: () => onBusy?.(false),
  });
  const cancel = useMutation({ mutationFn: () => sshApi.cancel(id) });
  const pending = test.isPending || trust.isPending;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          disabled={disabled || pending}
          onClick={() => test.mutate()}
        >
          {test.isPending ? 'Connecting…' : 'Test Connection'}
        </Button>
        {test.isPending && (
          <Button variant="ghost" onClick={() => cancel.mutate()}>
            Cancel
          </Button>
        )}
      </div>
      {(test.error || trust.error || cancel.error) && (
        <p role="alert" className="text-sm text-destructive">
          {(test.error || trust.error || cancel.error)?.message}
        </p>
      )}
      {test.data && (
        <p role="status" className="text-sm">
          {test.data.status}: {test.data.message}
          {test.data.latencyMs !== null && (
            <> ({Math.round(test.data.latencyMs)} ms full connection test)</>
          )}
        </p>
      )}
      {!lease && test.data?.errorCode === 'PASSPHRASE_REQUIRED' && (
        <Link className="text-primary underline" to={`/servers/${id}`}>
          Open details to enter the key passphrase
        </Link>
      )}
      <Dialog
        open={observation !== null}
        onOpenChange={(open) => {
          if (!open && !pending) setObservation(null);
        }}
      >
        <DialogContent>
          <DialogTitle>
            {observation?.previousFingerprint
              ? 'Security warning: host key changed'
              : 'Trust this SSH host key?'}
          </DialogTitle>
          <DialogDescription>
            {observation?.previousFingerprint
              ? 'Authentication was blocked. A changed key can indicate interception or a rebuilt server. Verify the new fingerprint through your hosting console before replacement.'
              : 'This is the first connection. Verify this fingerprint through your hosting provider or another trusted channel before trusting the server.'}
          </DialogDescription>
          <p className="break-all">
            {observation?.host}:{observation?.port}
          </p>
          <p>{observation?.algorithm}</p>
          {observation?.previousFingerprint && (
            <p className="break-all text-destructive">
              Previously trusted: {observation.previousFingerprint}
            </p>
          )}
          <p className="break-all font-mono text-sm">
            {observation?.fingerprint}
          </p>
          {observation?.previousFingerprint && !review ? (
            <Button variant="destructive" onClick={() => setReview(true)}>
              Review key replacement
            </Button>
          ) : (
            <>
              {observation?.previousFingerprint && (
                <label className="flex gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={verified}
                    onChange={(e) => setVerified(e.target.checked)}
                  />
                  I independently verified the new key and intend to replace the
                  trusted identity.
                </label>
              )}
              <Button
                disabled={
                  pending || (!!observation?.previousFingerprint && !verified)
                }
                variant={
                  observation?.previousFingerprint ? 'destructive' : 'default'
                }
                onClick={() => trust.mutate()}
              >
                {trust.isPending
                  ? 'Saving trust…'
                  : observation?.previousFingerprint
                    ? 'Replace trusted key'
                    : 'Trust and reconnect'}
              </Button>
            </>
          )}
          <Button
            variant="outline"
            disabled={pending}
            onClick={() => setObservation(null)}
          >
            Cancel
          </Button>
          {trust.error && (
            <p role="alert" className="text-destructive">
              {trust.error.message}
            </p>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
