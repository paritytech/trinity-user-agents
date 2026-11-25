import { PermissionAuthorizationRequest } from '@parity/truapi-host';

/**
 * Decode a permission slot's stored request.
 *
 * Uses the codec the package generates rather than a schema restating the union here:
 * the definition stays upstream's, so a variant added to the core cannot drift out of
 * sync with a copy. Returns `null` on bytes that will not decode — a damaged row is
 * dropped from a read rather than erroring the stream it is part of.
 */
export function parsePermissionAuthorizationRequest(value: unknown): PermissionAuthorizationRequest | null {
  if (!(value instanceof Uint8Array)) return null;

  try {
    return PermissionAuthorizationRequest.dec(value);
  } catch {
    return null;
  }
}
