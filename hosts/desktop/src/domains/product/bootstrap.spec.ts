import { filter, firstValueFrom } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { onDeviceOsPermissionBlockedSideEffect, permissionsUseCase } from './$usecase/permissions';
import { bootstrapPermissions } from './bootstrap';
import { coreStorageRepository } from './core-storage/repository';
import { _resetRemotePermissionBroker, pendingRemotePermissionRequests$ } from './permissions/broker';
import { _resetTransientDevicePermissionGrants, grantTransientDevicePermission } from './permissions/resource';
import { type DevicePermissionType, type PermissionStatus, type RemotePermissionIpcRequest } from './permissions/types';

type RemoteHandler = (request: RemotePermissionIpcRequest) => Promise<PermissionStatus>;
type DeviceHandler = (request: {
  productId: string;
  permission: DevicePermissionType;
  executable: 'app' | 'widget';
}) => Promise<PermissionStatus>;

let remoteHandler: RemoteHandler | undefined;
let deviceHandler: DeviceHandler | undefined;
let osAllowed = true;

// The gates ask the core. What the core answers is the use case's concern; these tests
// are about the IPC wiring and the prompt fall-through, so its answer is spied in place.
vi.spyOn(permissionsUseCase, 'setPermissionsAdapter').mockImplementation(() => {});
vi.spyOn(permissionsUseCase, 'getPermissionStatus').mockResolvedValue('ask');
// Bootstrap runs the descriptor backfill; the rows it would repair live in Dexie and
// nothing here is about them. Answering "no incomplete rows" keeps the real use case
// on the path while leaving the database out of it.
vi.spyOn(coreStorageRepository, 'descriptorCandidates').mockResolvedValue([]);

// The OS-block report is a DI side effect: a handler registered on it observes the real
// fan-out, so nothing has to stand in for the module that declares it.
const onDeviceOsPermissionBlocked = vi.fn();
onDeviceOsPermissionBlockedSideEffect.registerHandler({ available: () => true, body: onDeviceOsPermissionBlocked });

function remoteRequest(url: string): RemotePermissionIpcRequest {
  return { productId: 'p.dot', executable: 'app', request: { tag: 'Remote', url } };
}

beforeEach(() => {
  _resetRemotePermissionBroker();
  // Session-scoped grants are a plain in-memory set; every test starts with none
  // and the one that needs a grant states it with the real write below.
  _resetTransientDevicePermissionGrants();
  remoteHandler = undefined;
  deviceHandler = undefined;
  osAllowed = true;
  onDeviceOsPermissionBlocked.mockClear();
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
  (globalThis as any).window = {
    App: {
      onDevicePermissionRequest: vi.fn((handler: DeviceHandler) => {
        deviceHandler = handler;
      }),
      onRemotePermissionRequest: vi.fn((handler: RemoteHandler) => {
        remoteHandler = handler;
      }),
      requestSystemDevicePermission: vi.fn((_permission: string) => Promise.resolve(osAllowed)),
    },
  };
});

afterEach(() => {
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
  delete (globalThis as any).window;
});

// The core's permission API is injected; these tests are about the IPC wiring, not
// about what the core answers.
function stubPermissionsAdapter() {
  return {
    getStatus: vi.fn(() => Promise.resolve('NotDetermined' as const)),
    getStatuses: vi.fn(() => Promise.resolve([])),
    setStatus: vi.fn(() => Promise.resolve()),
  };
}

describe('bootstrapPermissions — remote permission gate', () => {
  it('prompts the user when the matching stored pattern is "ask"', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('ask');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    const pendingPromise = firstValueFrom(pendingRemotePermissionRequests$.pipe(filter(list => list.length > 0)));
    const decision = remoteHandler?.(remoteRequest('https://cdn.example.com/x.png'));

    const [prompt] = await pendingPromise;
    prompt?.resolve('granted');

    await expect(decision).resolves.toBe('granted');
  });

  it('prompts when overlapping matches roll up to "ask" (some granted, some ask)', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('ask');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    const pendingPromise = firstValueFrom(pendingRemotePermissionRequests$.pipe(filter(list => list.length > 0)));
    const decision = remoteHandler?.(remoteRequest('https://cdn.example.com/x.png'));

    const [prompt] = await pendingPromise;
    prompt?.resolve('denied');

    await expect(decision).resolves.toBe('denied');
  });

  it('returns a stored "granted" decision without prompting', async () => {
    const emitted: unknown[][] = [];
    const sub = pendingRemotePermissionRequests$.subscribe(list => emitted.push(list));
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(remoteHandler?.(remoteRequest('https://cdn.example.com/x.png'))).resolves.toBe('granted');

    expect(emitted.every(list => list.length === 0)).toBe(true);
    sub.unsubscribe();
  });

  it('returns a stored "denied" decision without prompting', async () => {
    const emitted: unknown[][] = [];
    const sub = pendingRemotePermissionRequests$.subscribe(list => emitted.push(list));
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('denied');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(remoteHandler?.(remoteRequest('https://cdn.example.com/x.png'))).resolves.toBe('denied');

    expect(emitted.every(list => list.length === 0)).toBe(true);
    sub.unsubscribe();
  });

  it('denies a stored "ask" without prompting when prompts are disabled', async () => {
    const emitted: unknown[][] = [];
    const sub = pendingRemotePermissionRequests$.subscribe(list => emitted.push(list));
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('ask');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: false });

    await expect(remoteHandler?.(remoteRequest('https://cdn.example.com/x.png'))).resolves.toBe('denied');

    expect(emitted.every(list => list.length === 0)).toBe(true);
    sub.unsubscribe();
  });
});

describe('bootstrapPermissions — device permission gate', () => {
  it('returns "granted" when a transient grant exists, without reading persisted status', async () => {
    grantTransientDevicePermission({
      productId: 'p.dot',
      permission: 'Camera',
      executionKind: 'App',
    });
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });
    // This file has no clearAllMocks in beforeEach; isolate read$'s call history so
    // the assertion measures only the device handler's behavior.
    vi.mocked(permissionsUseCase.getPermissionStatus).mockClear();

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Camera', executable: 'app' })).resolves.toBe('granted');
    expect(permissionsUseCase.getPermissionStatus).not.toHaveBeenCalled();
  });

  it('falls through to the core when there is no transient grant', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Camera', executable: 'app' })).resolves.toBe('granted');
  });

  it('returns "ask" when neither a transient grant nor a core decision exists', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('ask');
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Camera', executable: 'app' })).resolves.toBe('ask');
  });

  it('fires the OS-block side effect and denies when the OS blocks a granted Camera', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    osAllowed = false;
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Camera', executable: 'app' })).resolves.toBe('denied');
    expect(onDeviceOsPermissionBlocked).toHaveBeenCalledWith({ permission: 'Camera' });
  });

  it('grants a Camera the OS allows and fires no side effect', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    osAllowed = true;
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Camera', executable: 'app' })).resolves.toBe('granted');
    expect(onDeviceOsPermissionBlocked).not.toHaveBeenCalled();
  });

  it('does not OS-gate a non-media permission', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    osAllowed = false;
    bootstrapPermissions({ permissionsAdapter: stubPermissionsAdapter(), promptForUnmatchedRemoteAccess: true });

    await expect(deviceHandler?.({ productId: 'p.dot', permission: 'Bluetooth', executable: 'app' })).resolves.toBe('granted');
    expect(onDeviceOsPermissionBlocked).not.toHaveBeenCalled();
  });
});
