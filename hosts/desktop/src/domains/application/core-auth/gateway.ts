import {
  type WireProvider,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  VersionedHostRequestLoginRequest,
  decodeWireMessage,
  encodeWireMessage,
} from '@parity/truapi';
import { ACCOUNT_REQUEST_LOGIN } from '@parity/truapi/wire-table';

import { loginResponseCodec } from './schemas';

let requestSeq = 0;

/**
 * Ask the core to start a pairing over an already-open provider. Resolves with the
 * core's login outcome; the pairing deeplink itself arrives separately, through
 * `authStateChanged`.
 *
 * The provider is a parameter rather than something this gateway opens: it must be
 * the host's one reused auth provider, because each provider is a fresh core session
 * and a new one per attempt strands the previous pairing. Owning that lifetime is the
 * caller's job.
 */
function requestLogin(provider: WireProvider, reason?: string): Promise<string> {
  const requestId = `polkadot-desktop:login:${++requestSeq}`;
  const frame = encodeWireMessage({
    requestId,
    payload: {
      // Codec 2 addresses a frame by (trait, method) plus the leg it carries,
      // rather than by one discriminant per leg.
      traitId: ACCOUNT_REQUEST_LOGIN.trait,
      methodId: ACCOUNT_REQUEST_LOGIN.method,
      messageType: MESSAGE_TYPE_REQUEST,
      value: VersionedHostRequestLoginRequest.enc({ tag: 'V1', value: { reason } }),
    },
  });

  if (frame.isErr()) return Promise.reject(frame.error);

  return new Promise<string>((resolve, reject) => {
    let settled = false;

    const finish = (run: () => void) => {
      if (settled) return;
      settled = true;
      unsubscribeMessage();
      unsubscribeClose?.();
      run();
    };

    const unsubscribeMessage = provider.subscribe(message => {
      const decoded = decodeWireMessage(message);
      if (decoded.isErr()) {
        finish(() => reject(decoded.error));

        return;
      }

      const { requestId: id, payload } = decoded.value;
      if (
        id !== requestId ||
        payload.traitId !== ACCOUNT_REQUEST_LOGIN.trait ||
        payload.methodId !== ACCOUNT_REQUEST_LOGIN.method ||
        payload.messageType !== MESSAGE_TYPE_RESPONSE
      ) {
        return;
      }

      finish(() => {
        const result = loginResponseCodec.dec(payload.value);

        if (result.success) {
          resolve(result.value.value);
        } else {
          reject(new Error(`Login request failed: ${JSON.stringify(result.value)}`));
        }
      });
    });

    const unsubscribeClose = provider.subscribeClose?.(error => finish(() => reject(error)));

    provider.postMessage(frame.value);
  });
}

export const coreAuthGateway = {
  requestLogin,
};
