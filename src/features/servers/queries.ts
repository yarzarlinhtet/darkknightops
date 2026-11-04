import { queryOptions } from '@tanstack/react-query';
import { serversApi } from '@/lib/ipc';
export const serversQuery = queryOptions({
  queryKey: ['servers'],
  queryFn: serversApi.list,
  retry: false,
});

export const sshMetadataKey = (id: string) => ['ssh', id] as const;
