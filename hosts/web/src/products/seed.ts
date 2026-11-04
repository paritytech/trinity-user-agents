/**
 * Content a build ships beside the page: the blocks of one DotNS record's
 * content, fetched and checked when the host was built, at `/ipfs/<cid>` on
 * this host's origin. It is a cache filled ahead of time, not a source: a name
 * is still resolved on chain, and the shipped blocks are read only when the
 * chain points at exactly their CID. Each block is still checked against its
 * identifier as it is read.
 */
export interface ContentSeed {
  /** The DotNS record that pointed at the content when the build fetched it. */
  record: string;
  cid: string;
  kind: "car" | "site";
}

/** Where a build writes the description of its seed, beside the page. */
export const SEED_MANIFEST_PATH = "browse-seed.json";

/** This build's seed, or null for a build or dev server without one. */
export const SEED: ContentSeed | null =
  typeof __CONTENT_SEED__ === "undefined" ? null : __CONTENT_SEED__;

/** What `cid` holds when this build ships it, so a lookup need not ask the gateway. */
export function seededKind(
  cid: string,
  seed: ContentSeed | null = SEED,
): ContentSeed["kind"] | null {
  return seed?.cid === cid ? seed.kind : null;
}

/** Where `cid`'s blocks are read: this host's origin when the build ships them, otherwise `gateway`. */
export function blockGateway(
  cid: string,
  gateway: string,
  origin: string,
  seed: ContentSeed | null = SEED,
): string {
  return seed?.cid === cid ? origin : gateway;
}
