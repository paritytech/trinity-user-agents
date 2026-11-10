import { useRef } from 'react';

import { useConfirmation } from '@/shared/components';
import { useSideEffect } from '@/shared/di';
import { onDeviceOsPermissionBlockedSideEffect } from '@/domains/product';
import { PermissionDeniedDialog } from '../ui/PermissionDeniedDialog';

/**
 * Shows the system-privacy routing dialog when the domain reports that the OS blocked a
 * Camera/Microphone grant. Registered for the host's lifetime; the domain fires the side
 * effect only under Electron, so `window.App` is present when the settings button runs.
 */
export function useDeviceBlockedPrompt(): void {
  const confirm = useConfirmation();
  const nextId = useRef(0);

  useSideEffect(onDeviceOsPermissionBlockedSideEffect, ({ permission }) => {
    const id = `device-os-blocked:${nextId.current++}`;

    void confirm<void>(id, ({ resolve }) => (
      <PermissionDeniedDialog
        permission={permission}
        onOpenSettings={() => {
          void window.App?.openSystemPrivacySettings(permission);
        }}
        onClose={() => resolve()}
      />
    )).catch(() => {});
  });
}
