import { coreStorageUseCase } from './$usecase/coreStorage';
import { onDeviceOsPermissionBlockedSideEffect, permissionsUseCase } from './$usecase/permissions';
import { remoteAccessUseCase } from './$usecase/remoteAccess';
import { getTransientDevicePermissionGranted } from './permissions/resource';
import { permissionsService } from './permissions/service';
import { type DevicePermissionType, type PermissionStatus, type PermissionsAdapter } from './permissions/types';
import { type ExecutableKind } from './product/manifest/constants';
import { manifestService } from './product/manifest/service';

type BootstrapProductConfig = {
  /**
   * Behaviour when an external-URL request matches no stored permission:
   * `true` → prompt the user; `false` → deny silently. Automated/e2e runs pass
   * `false` so probes observe a fast 403 and UI tests aren't blocked by an
   * unexpected modal. The flag is injected by the app bootstrap — the domain
   * holds no knowledge of test environments.
   */
  promptForUnmatchedRemoteAccess: boolean;
  /**
   * The core's permission API. Injected because the runtime handle lives in an
   * aggregate and a domain may not import one.
   */
  permissionsAdapter: PermissionsAdapter;
};

// Registers the renderer's product-permission IPC request handlers on the host and
// applies the remote-access prompt policy. Called explicitly from the app bootstrap
// (never at import time), so the domain owns no hidden load-order side effects. The
// IPC handlers are registered only where the host bridge exists; the prompt policy is
// applied unconditionally (web build, node-env tests included).
export function bootstrapPermissions({ promptForUnmatchedRemoteAccess, permissionsAdapter }: BootstrapProductConfig): void {
  permissionsUseCase.setPermissionsAdapter(permissionsAdapter);
  void backfillCoreSlotDescriptors();

  // Apply the renderer-side chokepoint policy before the host-bridge guard: the worker
  // fetch resolver and navigateTo binding run `resolveRemoteUrlAccess` even on the web
  // build (no `window.App`), so the policy must be set there too — otherwise unmatched
  // URLs default to a silent deny instead of prompting.
  remoteAccessUseCase.setRemoteAccessPromptPolicy(promptForUnmatchedRemoteAccess);

  if (typeof window === 'undefined' || !window.App) return;

  const app = window.App;

  const resolveDeviceDecision = async ({
    productId,
    permission,
    executable,
  }: {
    productId: string;
    permission: DevicePermissionType;
    executable: ExecutableKind;
  }): Promise<PermissionStatus> => {
    // A live "allow once" grant for this session opens the native gate without a
    // persisted 'granted'. Checked before the persisted lookup (and before any IO).
    if (
      getTransientDevicePermissionGranted({ productId, permission, executionKind: manifestService.executionKindOf(executable) })
    ) {
      return 'granted';
    }
    // The core owns the persisted answer. A settings id maps to at most one core
    // request; an id it cannot build is one the core never stores, so 'ask'.
    const settingsId = permissionsService.getSettingsPermissionId(permission);
    const request = permissionsService.isPermissionId(settingsId) ? permissionsService.toAuthorizationRequest(settingsId) : null;
    if (!request) return 'ask';

    // Bounded and fail-closed inside the use case: this gate opens a camera and a
    // microphone, so a core that never answers must deny, never hang open.
    return permissionsUseCase.getPermissionStatus({ productId, request });
  };

  app.onDevicePermissionRequest(async ({ productId, permission, executable }) => {
    const decision = await resolveDeviceDecision({ productId, permission, executable });
    if (decision !== 'granted' || !permissionsService.isOsGatedDevicePermission(permission)) {
      return decision;
    }

    // The user/core granted, but macOS can still have the device blocked. `false` means
    // denied/restricted (it does not re-prompt in that state), so surface the routing
    // dialog and deny. A silently failing capture is the gap this closes.
    const osAllowed = await app.requestSystemDevicePermission(permission);
    if (osAllowed) return 'granted';

    void onDeviceOsPermissionBlockedSideEffect.apply({ permission });

    return 'denied';
  });

  app.onRemotePermissionRequest(async ({ productId, executable, request }) => {
    if (request.tag !== 'Remote') return 'denied';

    return remoteAccessUseCase.resolveRemoteUrlAccess({
      productId,
      url: request.url,
      executionKind: manifestService.executionKindOf(executable),
    });
  });
}

// Wires the product domain to the host environment (IPC request handlers, etc.).
// Composes the per-module bootstraps and is the domain's public entry point —
// call it once from the app bootstrap, never at module-import time, so host
// wiring and its load order stay explicit.
export function bootstrapProduct(config: BootstrapProductConfig): void {
  bootstrapPermissions(config);
}

// Best-effort at bootstrap: a failed descriptor backfill must not abort host wiring, so
// it is caught and logged. The backfill itself (why it is a domain concern, its
// idempotency) is documented on `coreStorageUseCase.backfillDescriptors`.
async function backfillCoreSlotDescriptors(): Promise<void> {
  try {
    await coreStorageUseCase.backfillDescriptors();
  } catch (error) {
    console.warn('[permissions] core storage descriptor backfill failed', error);
  }
}
