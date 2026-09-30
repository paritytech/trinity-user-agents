/** The cache the archive's files are kept in, by path, on the product's own origin. */
export const ARCHIVE_CACHE = "truapi-archive";

/** The address a file is stored under: never a path the browser could serve on its own. */
export function fileKey(origin: string, path: string): string {
  return `${origin}/__files/${encodeURIComponent(path)}`;
}
