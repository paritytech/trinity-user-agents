import { type AuthState } from '@parity/truapi-host';

import { createState } from '@/shared/rxstate';
import { type TruapiRuntimeState } from '../types';

/**
 * The single TrUAPI core runtime and the auth state it projects.
 *
 * `authState` is `null` until the core emits for the first time. That is not the
 * same as `Disconnected`: the core resolves asynchronously from a Web Worker, and
 * treating "not answered yet" as "signed out" is what bounces an already-paired
 * user through onboarding on cold start.
 */
export const truapiRuntime = createState<TruapiRuntimeState>({ runtime: null, authState: null, loggingOut: false });

export function setAuthState(authState: AuthState): void {
  truapiRuntime.set(prev => ({ ...prev, authState }));
}

/** Latch the logout-in-progress flag. Cleared only by the reload that ends the logout. */
export function markLoggingOut(): void {
  truapiRuntime.set(prev => ({ ...prev, loggingOut: true }));
}
