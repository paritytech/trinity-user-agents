/** Sent with every HTML page a product is served, since a browser has no other way to stop these. */
export const PRODUCT_CSP = [
  "frame-src 'self'",
  "object-src 'none'",
  "base-uri 'self'",
  "form-action 'self'",
  "worker-src 'none'",
].join("; ");

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

const AFTER_DOCTYPE = /^(\s*(?:<!--[\s\S]*?-->\s*)*<!doctype[^>]*>)/i;

/**
 * `html` with the container loaded first, as a plain synchronous script.
 *
 * It goes straight after the doctype, or at the very start without one, so no
 * script of the product's can run before it whatever the page puts in its head.
 * The host's origin is written into the page, which pins where the container
 * accepts its private port from. `src` is the container's absolute path.
 */
export function injectContainer(
  html: string,
  hostOrigin: string,
  src: string,
): string {
  const flag = JSON.stringify(hostOrigin).replaceAll("<", "\\u003c");
  const scripts =
    `<script>window.__truapi_message_port=${flag}</script>` +
    `<script src="${src}"></script>`;
  const doctype = AFTER_DOCTYPE.exec(html);
  if (doctype === null) return scripts + html;
  return doctype[1] + scripts + html.slice(doctype[1].length);
}

/** What the worker answers a request with: found, or not. */
export interface ServedFile {
  path: string;
  bytes: Uint8Array;
}

/**
 * The response for an archive file, with the container injected into HTML.
 * A null `container` serves the page as it is, without the container or the
 * policy that goes with it.
 */
export function responseFor(
  file: ServedFile,
  container: { hostOrigin: string; src: string } | null,
): Response {
  const type = contentTypeOf(file.path);
  const headers: Record<string, string> = {
    "content-type": type,
    "x-content-type-options": "nosniff",
    "cache-control": "no-store",
  };
  if (type.startsWith("text/html") && container !== null) {
    headers["content-security-policy"] = PRODUCT_CSP;
    const html = injectContainer(
      new TextDecoder().decode(file.bytes),
      container.hostOrigin,
      container.src,
    );
    return new Response(html, { headers });
  }
  return new Response(file.bytes as BodyInit, { headers });
}
