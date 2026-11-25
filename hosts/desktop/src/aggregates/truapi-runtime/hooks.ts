import { type ProductAccountId } from '@parity/truapi';
import { type AuthState, type SessionUiInfo } from '@parity/truapi-host';

import { useRead } from '@/shared/hooks';
import { useRxState } from '@/shared/rxstate';
import { type AccountId } from '@/domains/network';

import { productAccountUseCase } from './productAccountUseCase';
import { truapiRuntime } from './state/runtime';
import { truapiRuntimeUseCase } from './truapiRuntimeUseCase';

// Typed so `useRead` infers `string | null` from the default rather than `null`.
const NO_ADDRESS: string | null = null;

/**
 * The core's auth state, or `null` while the worker has not answered yet.
 *
 * Callers MUST treat `null` as "unknown", not "signed out" — rendering a signed-out
 * surface on `null` flashes onboarding at an already-paired user on every start.
 */
export function useTruapiAuthState(): AuthState | null {
  const [state] = useRxState(truapiRuntime);

  return state.authState;
}

/**
 * The connected session, or `null` when the core has not reported one.
 *
 * This is the app's single source of session truth — the host keeps no other.
 */
export function useTruapiSession(): SessionUiInfo | null {
  const [state] = useRxState(truapiRuntime);

  return state.authState?.tag === 'Connected' ? state.authState.value : null;
}

/** The signed-in user's account id, or `null` when no session is connected. */
export function useTruapiUserId(): AccountId | null {
  const [state] = useRxState(truapiRuntime);

  return truapiRuntimeUseCase.toUserId(state.authState);
}

/**
 * True while a full user logout is tearing down, until the renderer reloads.
 *
 * The authenticated shell renders a splash over itself on this, so the teardown's
 * intermediate signed-out renders and the onboarding redirect never flash.
 */
export function useLoggingOut(): boolean {
  const [state] = useRxState(truapiRuntime);

  return state.loggingOut;
}

/**
 * The SS58 address of a product account. `pending` is true while it derives; a settled
 * `address` of `null` means the core has not cached the product's subtree key so it
 * cannot be derived. Idle (not pending, `null`) until an account is given.
 *
 * The review modals show it in place of the raw derivation path; a legacy-account
 * review already carries its address and does not need this.
 */
export function useProductAccountAddress(account: Nullable<ProductAccountId>): { address: string | null; pending: boolean } {
  const { data, pending } = useRead(
    (params: { account: ProductAccountId }) => productAccountUseCase.deriveAddress(params.account),
    {
      params: account ? { account } : null,
      defaultValue: NO_ADDRESS,
    },
  );

  return { address: data, pending: account ? pending : false };
}
