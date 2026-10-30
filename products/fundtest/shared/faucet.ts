// The public Paseo faucet the provider funds its simulated payments from, as
// both executables need it: the worker calls it, and a screen asks the user
// for network access to it while the user is there to answer.

import type { RemotePermissionRequest } from "@parity/truapi";

/** The faucet's drip endpoint. */
export const FAUCET_URL =
  "https://paseo-faucet.parity-testnet.parity.io/drip/web";

/** The `Remote` permission that lets this product reach the faucet. Grants
 *  are kept per product, so one made on a screen covers the worker too. */
export const FAUCET_PERMISSION: RemotePermissionRequest = {
  permission: {
    tag: "Remote",
    value: { domains: [new URL(FAUCET_URL).hostname] },
  },
};
