/**
 * The network this host serves unless a caller names another.
 *
 * A network id is a stable, lower-case token the host config gives each
 * network, such as `paseo` and `previewnet`. It is not the dotNS TLD: the TLD
 * can change with a network's naming, while the id is what storage keys and
 * mount paths are built from.
 */
export const DEFAULT_NETWORK = "paseo";

/**
 * A network id is safe in a storage key and a URL path: lower-case letters,
 * digits and hyphens.
 */
const NETWORK_ID = /^[a-z0-9-]+$/;

/**
 * The canonical form of `network`: trimmed and lower-cased. Throws when it
 * cannot be a network id, because a wrong one would quietly read and write
 * another network's data.
 */
export function normalizeNetwork(network: string): string {
  const id = network.trim().toLowerCase();
  if (!NETWORK_ID.test(id)) throw new Error(`"${network}" is not a network id.`);
  return id;
}