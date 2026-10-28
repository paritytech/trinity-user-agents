import {
  classifyContent,
  httpRpc,
  resolveContentCid,
  resolveTextRecord,
  type ContentKind,
  type RpcCall,
} from "../products/dotns.js";
import type { NetworkConfig } from "../platform/network-config.js";
import type { WorkerReaders } from "../products/worker-manifest.js";

/**
 * The content an opened Pocket card runs as the product's Widget.
 *
 * The rule is the iOS host's (`ProductResolver` and
 * `ResolvedProduct.contentId(for: .widget)`):
 *
 * - The base name's `manifest` text record is the root manifest. One that is
 *   present and not valid stops here: the product published something, and
 *   nothing else is served in its place.
 * - `widget.<name>` with a valid widget manifest serves its own `contenthash`.
 * - A product that publishes no widget manifest, or no root manifest at all,
 *   has its Widget served from the base name's `contenthash`. The app's
 *   `app.<name>` is never read.
 *
 * Where this host is stricter than iOS: a `widget.<name>` manifest that is
 * present but not valid is refused with the reason, where iOS skips it and
 * serves the base name.
 */
export interface ResolvedWidget {
  /** The DotNS record the content came from: `widget.<name>`, or `<name>`. */
  record: string;
  cid: string;
  kind: Exclude<ContentKind, "other">;
  /** Whether the base name serves it because the product publishes no widget. */
  fromBaseName: boolean;
}

function parseObject(json: string, what: string): Record<string, unknown> {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    throw new Error(`the ${what} is not JSON`);
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw))
    throw new Error(`the ${what} is not an object`);
  return raw as Record<string, unknown>;
}

/**
 * Check a root manifest the way the iOS host does: `$v` 1, a `displayName`,
 * a `description`, and an icon with a `cid` and a `format`. Throws with the
 * reason when it is not one.
 */
export function parseRootManifest(json: string): void {
  const record = parseObject(json, "root manifest");
  if (record.$v !== 1)
    throw new Error(`manifest version ${String(record.$v)} is not 1`);
  if (typeof record.displayName !== "string" || record.displayName === "")
    throw new Error("displayName is missing");
  if (typeof record.description !== "string")
    throw new Error("description is missing");
  const icon = record.icon as { cid?: unknown; format?: unknown } | undefined;
  if (typeof icon?.cid !== "string" || icon.cid === "")
    throw new Error("icon.cid is missing");
  if (typeof icon.format !== "string")
    throw new Error("icon.format is missing");
}

/**
 * Check a `widget.<name>` executable manifest. Throws with the reason when it
 * is not one: the wrong `$v` or `kind`, or no heights to render at.
 */
export function parseWidgetManifest(json: string): void {
  const record = parseObject(json, "executable record");
  if (record.$v !== 1)
    throw new Error(`manifest version ${String(record.$v)} is not 1`);
  if (record.kind !== "widget")
    throw new Error(`the executable is a ${String(record.kind)}, not a widget`);
  const heights = (record.dimensions as { height?: unknown } | undefined)
    ?.height;
  if (!Array.isArray(heights) || heights.length === 0)
    throw new Error("dimensions.height lists no height");
}

const reason = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

/** Find the content `name`'s Widget runs. Throws with what is missing or wrong. */
export async function findWidget(
  name: string,
  readers: WorkerReaders,
): Promise<ResolvedWidget> {
  const root = await readers.readText(name, "manifest");
  if (root !== null && root !== "") {
    try {
      parseRootManifest(root);
    } catch (error) {
      throw new Error(
        `${name}'s root manifest is not usable: ${reason(error)}, so its card does not open.`,
      );
    }
  }
  const widgetRecord = `widget.${name}`;
  const manifest =
    root !== null && root !== ""
      ? await readers.readText(widgetRecord, "executable")
      : null;
  let record = name;
  if (manifest !== null && manifest !== "") {
    try {
      parseWidgetManifest(manifest);
    } catch (error) {
      throw new Error(
        `${widgetRecord}'s manifest is not usable: ${reason(error)}.`,
      );
    }
    record = widgetRecord;
  }
  const cid = await readers.readCid(record);
  if (cid === null)
    throw new Error(
      record === name
        ? `${name} publishes no widget and has no content of its own, so its card does not open.`
        : `${record} has no content.`,
    );
  const kind = await readers.classify(cid);
  if (kind === "other")
    throw new Error(
      `${record}'s content is neither an archive nor a directory.`,
    );
  return { record, cid, kind, fromBaseName: record === name };
}

/** Find `name`'s widget on `network`'s Asset Hub and Bulletin gateway. */
export function resolveWidget(
  name: string,
  network: NetworkConfig,
  rpc: RpcCall = httpRpc(network.dotns.assetHubRpc),
): Promise<ResolvedWidget> {
  return findWidget(name, {
    readCid: (record) => resolveContentCid(record, rpc, network.dotns),
    readText: (record, key) =>
      resolveTextRecord(record, key, rpc, network.dotns),
    classify: (cid) =>
      classifyContent(cid, fetch, network.dotns.contentGateway),
  });
}
