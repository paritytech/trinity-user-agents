/**
 * The cache holding one archive's files. It is named by the content id, so it
 * holds the same bytes for every wallet and every tab that opens that content,
 * and nothing mutable lives in it.
 */
export function archiveCacheName(cid: string): string {
  return `truapi-archive:${cid}`;
}

/** The address a file is stored under: never a path the browser could serve on its own. */
export function fileKey(origin: string, path: string): string {
  return `${origin}/__files/${encodeURIComponent(path)}`;
}

/** Written after every file, so a cache without it is a load that did not finish. */
export function completeKey(origin: string): string {
  return `${origin}/__complete`;
}
