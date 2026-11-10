/**
 * Consumer-side view of device sync. The orchestrator reports every peer's
 * connection phase here uniformly; this module keeps only the phase of the
 * single *tracked* peer (set by wiring) plus a persisted `lastConnectionClosedAt`
 * timestamp, and derives the semantic {@link DeviceSyncStatus} for the UI.
 */

import { type Observable, BehaviorSubject, from, interval, map, merge, of, startWith, switchMap } from 'rxjs';

import { createStreamResource } from '@/shared/resource';

import { DEVICE_SYNC_UI_TICK_MS } from './constants';
import { deviceSyncRepository } from './repository';
import { deviceSyncService } from './service';
import { type DeviceSyncConnectionPhase, type DeviceSyncStatus } from './types';

const connectionPhase$ = new BehaviorSubject<DeviceSyncConnectionPhase>('inactive');

// Which peer's phase drives the UI status. The orchestrator is peer-agnostic and
// reports every peer; selection of the one peer the user cares about (the
// PApp/mobile device) is a consumer concern, set by wiring.
let trackedPeerId: string | null = null;

/** Select the peer whose connection phase the UI status reflects. `null` resets to inactive. */
export const setTrackedSyncPeer = (peerId: string | null): void => {
  trackedPeerId = peerId;
  if (peerId === null) {
    connectionPhase$.next('inactive');
  }
};

/**
 * Report a peer's connection phase. Ignored unless it's the tracked peer; for the
 * tracked peer it drives the phase stream and the persisted staleness timestamp.
 */
export const reportPeerSyncPhase = (peerId: string, phase: DeviceSyncConnectionPhase): void => {
  if (peerId !== trackedPeerId) return;
  connectionPhase$.next(phase);
  if (phase === 'disconnected') {
    void deviceSyncRepository.setLastConnectionClosedAt(Date.now());
  } else if (phase === 'synced') {
    void deviceSyncRepository.clearLastConnectionClosedAt();
  }
};

const readStatus$ = (): Observable<DeviceSyncStatus> =>
  from(deviceSyncRepository.getConnectionMeta()).pipe(
    map(meta => deviceSyncService.resolveDeviceSyncStatus(connectionPhase$.value, meta.lastConnectionClosedAt, Date.now())),
  );

export const deviceSyncStatusResource = createStreamResource<object>({
  key: () => 'device-sync-status',
})
  .subscribe<DeviceSyncStatus>(() =>
    merge(
      connectionPhase$.pipe(map(() => undefined)),
      interval(DEVICE_SYNC_UI_TICK_MS).pipe(
        startWith(0),
        map(() => undefined),
      ),
    ).pipe(switchMap(() => readStatus$())),
  )
  .mock(() => of<DeviceSyncStatus>('inactive'))
  .cache<DeviceSyncStatus>({
    initial: 'inactive',
    map: (_cache, status) => status,
  })
  .build();
