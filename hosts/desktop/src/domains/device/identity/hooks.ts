import { useEffect, useState } from 'react';

import { deviceIdentityUseCase } from '../$usecase/identity';

import { type DeviceIdentity } from './types';

/**
 * This device's identity, or `null` until the first read resolves.
 *
 * Minting is a one-off async read, not a subscription: the identity is stable for
 * the install, so there is nothing to watch. Callers MUST treat `null` as "not read
 * yet" rather than "no identity" — every install has one.
 */
export function useDeviceIdentity(): DeviceIdentity | null {
  const [identity, setIdentity] = useState<DeviceIdentity | null>(null);

  useEffect(() => {
    let active = true;

    deviceIdentityUseCase
      .getDeviceIdentity()
      .then(value => {
        if (active) setIdentity(value);
      })
      .catch((error: unknown) => {
        console.error('[device-identity] failed to load', error);
      });

    return () => {
      active = false;
    };
  }, []);

  return identity;
}
