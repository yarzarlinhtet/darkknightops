import * as React from 'react';
import * as Primitive from '@radix-ui/react-alert-dialog';
import { cn } from '@/lib/utils';
export const AlertDialog = Primitive.Root;
export const AlertDialogTitle = Primitive.Title;
export const AlertDialogDescription = Primitive.Description;
export const AlertDialogCancel = Primitive.Cancel;
export function AlertDialogContent({
  className,
  ...props
}: React.ComponentProps<typeof Primitive.Content>) {
  return (
    <Primitive.Portal>
      <Primitive.Overlay className="fixed inset-0 z-40 bg-black/60" />
      <Primitive.Content
        className={cn(
          'fixed left-1/2 top-1/2 z-50 w-[calc(100%-2rem)] max-w-lg -translate-x-1/2 -translate-y-1/2 rounded-xl border bg-card p-6 shadow-xl',
          className,
        )}
        {...props}
      />
    </Primitive.Portal>
  );
}
