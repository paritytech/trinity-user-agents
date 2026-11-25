import { VersionedHostRequestLoginError, VersionedHostRequestLoginResponse, scale } from '@parity/truapi';

/**
 * The core's `Account/request_login` response payload.
 *
 * Codec 2 moved the version tag inside the `Result` — a response leg is
 * `Result<V<N>(Response), CallError<V<N>(Error)>>`, not `V<N>(Result<…>)` as it
 * was through 0.13. Mirrors the generated caller in
 * `@parity/truapi/dist/generated/client.js`.
 */
export const loginResponseCodec = scale.Result(
  VersionedHostRequestLoginResponse,
  scale.CallError(VersionedHostRequestLoginError),
);
