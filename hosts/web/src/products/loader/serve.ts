const CONTENT_TYPES: Record<string, string> = {
  html: "text/html; charset=utf-8",
  htm: "text/html; charset=utf-8",
  js: "text/javascript; charset=utf-8",
  mjs: "text/javascript; charset=utf-8",
  css: "text/css; charset=utf-8",
  json: "application/json",
  map: "application/json",
  webmanifest: "application/manifest+json",
  txt: "text/plain; charset=utf-8",
  svg: "image/svg+xml",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  avif: "image/avif",
  ico: "image/x-icon",
  woff: "font/woff",
  woff2: "font/woff2",
  ttf: "font/ttf",
  otf: "font/otf",
  wasm: "application/wasm",
  mp3: "audio/mpeg",
  mp4: "video/mp4",
  webm: "video/webm",
};

/** The content type for a file, by extension. Unknown extensions are not guessed at. */
export function contentTypeOf(path: string): string {
  const extension = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  return CONTENT_TYPES[extension] ?? "application/octet-stream";
}

/**
 * The archive path a request names, or null when it names none.
 *
 * `/` and any directory-style path resolve to its `index.html`. A segment that
 * climbs, hides a backslash or holds a NUL never resolves, so a request cannot
 * ask for anything outside the archive's own file names.
 */
export function archivePath(pathname: string): string | null {
  let decoded: string;
  try {
    decoded = decodeURIComponent(pathname);
  } catch {
    return null;
  }
  const trimmed = decoded.replace(/^\/+/, "");
  const path =
    trimmed === "" || trimmed.endsWith("/") ? `${trimmed}index.html` : trimmed;
  const segments = path.split("/");
  if (
    segments.some(
      (segment) => segment === "" || segment === "." || segment === "..",
    )
  )
    return null;
  return /[\\\0]/.test(path) ? null : path;
}

/**
 * The archive path a request from a mounted product names, or null for none.
 *
 * A product's own relative links arrive under its mount path, and the mount
 * path is stripped. Root-relative links, such as `/assets/app.js` from a build
 * made for the site root, arrive as they are and name the archive path
 * directly, because the worker answers every request its page makes, not only
 * those under its scope.
 */
export function archiveRequestPath(
  pathname: string,
  scopePath: string,
): string | null {
  return archivePath(
    pathname.startsWith(scopePath)
      ? `/${pathname.slice(scopePath.length)}`
      : pathname,
  );
}

/** What the worker answers a request with: found, or not. */
export interface ServedFile {
  path: string;
  bytes: Uint8Array;
}

/** The response for an archive file: its type by extension, never sniffed, never cached by the browser. */
export function responseFor(file: ServedFile): Response {
  return new Response(file.bytes as BodyInit, {
    headers: {
      "content-type": contentTypeOf(file.path),
      "x-content-type-options": "nosniff",
      "cache-control": "no-store",
    },
  });
}

/**
 * The page a worker executable runs in, at the root of its mount. A worker
 * archive is JavaScript only, so the service worker answers this one reserved
 * path itself, with a page that loads the host's console capture
 * (`console.ts`), then the entry module, and nothing else.
 */
export const WORKER_SHELL = "__truapi-worker.html";

/** The console capture's path under the host's base, where the build emits it. */
export const CONSOLE_CAPTURE = "truapi-sandbox/console.js";

/**
 * The shell page for `entry`, or null when `entry` is not a module path
 * inside the archive. The path is checked like any other request and encoded
 * segment by segment, so it cannot leave the attribute or the mount. `base`
 * is the host's base path, where the console capture is served from. The
 * capture is a classic script, so it runs before the module starts.
 */
export function workerShellHtml(entry: string, base: string): string | null {
  if (archivePath(`/${entry}`) !== entry || !/\.m?js$/.test(entry)) return null;
  const encode = (path: string) =>
    path.split("/").map(encodeURIComponent).join("/");
  return `<!doctype html>
<meta charset="utf-8">
<title>worker</title>
<script src="${encode(`${base}${CONSOLE_CAPTURE}`)}"></script>
<script type="module" src="./${encode(entry)}"></script>
`;
}
