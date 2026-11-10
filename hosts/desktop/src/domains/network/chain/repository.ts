import { type RemoteChain } from './schemas';

const LAST_KNOWN_CHAINS_KEY = 'pb:last-known-chains';

// The raw `chains_v2` payload of the last successful fetch, kept so a boot that
// cannot reach Remote Config still has a catalog. Raw rather than transformed:
// `chainService.fromRemoteChains` stays the single transform, and the blob is
// re-validated through the same schema on the way back out.
function persistLastKnownChains(raw: RemoteChain[]): void {
  try {
    localStorage.setItem(LAST_KNOWN_CHAINS_KEY, JSON.stringify(raw));
  } catch (error) {
    console.warn('[network] could not persist the last-known chain catalog', error);
  }
}

// Unvalidated — the caller parses it through `remoteChainsSchema`, so a blob
// written by an older shape is rejected there rather than trusted here.
function readLastKnownChains(): unknown {
  try {
    const raw = localStorage.getItem(LAST_KNOWN_CHAINS_KEY);

    return raw ? JSON.parse(raw) : null;
  } catch (error) {
    console.warn('[network] could not read the last-known chain catalog', error);

    return null;
  }
}

export const chainRepository = {
  persistLastKnownChains,
  readLastKnownChains,
};
