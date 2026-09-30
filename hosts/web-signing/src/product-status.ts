import type { ContainerState } from "./container-channel.js";
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
  /** The origin the content is served from, when opened by name. */
  origin?: string;
  /** The DotNS record the content came from, when opened by name. */
  record?: string;
  /** Records that exist for the name but could not be opened. */
  skipped?: SkippedRecord[];
}

/** What the status text reads of an open product. */
export interface ProductView {
  url: URL;
  /** The frame loaded a new document and its channels to the core were closed. */
  lost: boolean;
  container: { state: ContainerState } | null;
}

/** The container line: short, and blunt when nothing gates the page. */
export function containerLine(open: ProductView, how: Opened | null): string {
  if (open.lost) return "Container: ended. The page loaded a new document.";
  if (open.container?.state === "connected") return "Container: on";
  if (open.container)
    return how?.via === "name"
      ? "Container: starting"
      : "Container: waiting for the page. Not gated until it connects.";
  if (how?.via === "name")
    return "Container: off (relaxed). Requests are not gated.";
  return "Container: none. The page is not gated.";
}

/**
 * Camera and microphone. A page that is not a secure context, and so every
 * frame under it, has no `navigator.mediaDevices`.
 */
export function deviceLine(open: ProductView, secureContext: boolean): string {
  if (!secureContext)
    return "Camera/mic: unavailable. Needs https or localhost.";
  if (open.lost)
    return "Camera/mic: unavailable until the product is reopened.";
  if (open.container?.state === "connected")
    return "Camera/mic: the core asks, then the browser.";
  if (open.container)
    return "Camera/mic: allowed, but only the browser asks until the container connects.";
  return "Camera/mic: blocked.";
}

/** What to do about a product that navigated away from the connection the host gave it. */
const REOPEN_LINE =
  "The page navigated on its own, so the host's connection to it ended. Press Reopen.";

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
    ...(open.lost ? [REOPEN_LINE] : []),
    ...(sandboxStatus ? [sandboxStatus] : []),
    containerLine(open, how),
    deviceLine(open, secureContext),
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
    `Origin: ${how.origin ?? open.url.origin} (not this host's)`,
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
