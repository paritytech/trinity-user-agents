import { createState } from '@/shared/rxstate';

// Whether a call window is currently open, so the main app can show a
// return-to-call bar in the call peer's chat view while the call runs in its
// separate window. Single active call is assumed (matches the bridge's single-port TODO).
// sessionId scopes the bar to the peer's conversation; undefined shows it on any open chat.
export type CallActivity = { status: 'idle' } | { status: 'active'; peerName: string; sessionId: Nullable<string> };

export const callActivityState = createState<CallActivity>({ status: 'idle' });

export function setCallActive(peerName: string, sessionId: Nullable<string>): void {
  callActivityState.set({ status: 'active', peerName, sessionId });
}

export function setCallIdle(): void {
  callActivityState.set({ status: 'idle' });
}
