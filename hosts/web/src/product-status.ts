import { PASEO_DOTNS, type SkippedRecord } from "./dotns.js";

/** How the open product was reached, for the bar and the status line. */
export interface Opened {
  /** What the address bar shows for it: a URL or a product name. */
  address: string;
  /** `name` when a dotNS name was resolved to reach `url`. */
  via: "url" | "name";
  cid?: string;
  /** What the content is, when opened by name: an archive or a directory. */
  kind?: "car" | "site";
  /** The path the content is served under, on this host's origin, when opened by name. */
  mount?: string;
  /** The DotNS record the content came from, when opened by name. */
  record?: string;
  /** Records that exist for the name but could not be opened. */
  skipped?: SkippedRecord[];
}

/** What the status text reads of an open product. */
export interface ProductView {
  url: URL;
}

/**
 * Camera and microphone. A page that is not a secure context, and so every
 * frame under it, has no `navigator.mediaDevices`.
 */
export function deviceLine(secureContext: boolean): string {
  return secureContext
    ? "Camera/mic: the browser and the OS ask."
    : "Camera/mic: unavailable. Needs https or localhost.";
}

export function productStatusText(
  open: ProductView,
  how: Opened | null,
  sandboxStatus: string,
  secureContext: boolean,
): string {
  const lines = [
    how?.via === "name"
      ? `${how.address} (${how.kind === "car" ? "app archive" : "website"})`
      : `${open.url.href}`,
    ...(sandboxStatus ? [sandboxStatus] : []),
    deviceLine(secureContext),
  ];
  return lines.join("\n");
}

/** The longer facts behind a name: what was read, from where, and what is checked. */
export function productDetailsText(
  open: ProductView,
  how: Opened | null,
): string {
  if (how?.via !== "name" || how.cid === undefined) return "";
  const skipped = (how.skipped ?? []).map(
    ({ record, cid }) => `Skipped ${record} (${cid}): not a page or archive.`,
  );
  return [
    `Record: ${how.record ?? how.address}`,
    `CID: ${how.cid}`,
    ...skipped,
    `Served from: ${how.mount ?? open.url.pathname} on this host's origin, for this wallet and product only`,
    `Blocks: each checked against its CID. Gateway: ${PASEO_DOTNS.contentGateway} (not trusted).`,
    `Name: read from ${PASEO_DOTNS.assetHubRpc} (trusted, unverified).`,
  ].join("\n");
}

/** Why a name could not be opened, naming the records that do exist. */
export function notOpenableText(
  name: string,
  skipped: SkippedRecord[],
): string {
  if (skipped.length === 0)
    return `${name} has no content. It is not registered, or nothing is published.`;
  const listed = skipped.map(({ record }) => record).join(" and ");
  return `${listed} is neither a page nor an app archive. Serve it and enter its URL.`;
}
