import type { NativeChatPickedFile } from "../generated/host-callbacks.js";

type AttachmentMetadata = NativeChatPickedFile["metadata"];
type ImageHeader = { mimeType: string; width: number; height: number };
const HEADER_LIMIT = 65_536;
const VIDEO_TIMEOUT_MS = 5_000;
const U32_MAX = 0xffff_ffff;
const MP4_BRANDS = ["isom", "iso2", "iso3", "iso4", "iso5", "iso6", "mp41", "mp42", "avc1", "M4V ", "M4VH", "M4VP"];
const PNG_DEPTHS: Readonly<Record<number, readonly number[]>> = {
  0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16],
};

function matches(bytes: Uint8Array, offset: number, signature: string): boolean {
  if (offset + signature.length > bytes.length) return false;
  for (let i = 0; i < signature.length; i++) {
    if (bytes[offset + i] !== signature.charCodeAt(i)) return false;
  }
  return true;
}

/** Only inspect bounded headers; never render images, XML, SVG or HTML. */
function imageHeader(bytes: Uint8Array, fileSize: number): ImageHeader | undefined {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length >= 33 && matches(bytes, 0, "\x89PNG\r\n\x1a\n") && view.getUint32(8) === 13 && matches(bytes, 12, "IHDR")) {
    const width = view.getUint32(16);
    const height = view.getUint32(20);
    if (width > 0 && height > 0 && width <= 0x7fff_ffff && height <= 0x7fff_ffff && PNG_DEPTHS[bytes[25]!]?.includes(bytes[24]!) && bytes[26] === 0 && bytes[27] === 0 && bytes[28]! <= 1) {
      return { mimeType: "image/png", width, height };
    }
    return undefined;
  }
  if (bytes.length >= 13 && (matches(bytes, 0, "GIF87a") || matches(bytes, 0, "GIF89a"))) {
    const width = view.getUint16(6, true);
    const height = view.getUint16(8, true);
    if (width && height) return { mimeType: "image/gif", width, height };
    return undefined;
  }
  if (bytes.length >= 4 && bytes[0] === 0xff && bytes[1] === 0xd8) {
    let offset = 2;
    while (offset + 4 <= bytes.length) {
      if (bytes[offset++] !== 0xff) return undefined;
      while (offset < bytes.length && bytes[offset] === 0xff) offset++;
      const marker = bytes[offset++];
      // Dimensions must precede compressed scan data; never search arbitrarily inside it.
      if (marker === undefined || marker === 0xda || marker === 0xd9 || marker === 0x00) return undefined;
      if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) continue;
      if (offset + 2 > bytes.length) return undefined;
      const length = view.getUint16(offset);
      if (length < 2 || offset + length > bytes.length) return undefined;
      if (marker >= 0xc0 && marker <= 0xcf && marker !== 0xc4 && marker !== 0xc8 && marker !== 0xcc) {
        if (length < 8) return undefined;
        const height = view.getUint16(offset + 3);
        const width = view.getUint16(offset + 5);
        const components = bytes[offset + 7]!;
        if (width && height && components > 0 && length === 8 + 3 * components) {
          return { mimeType: "image/jpeg", width, height };
        }
        return undefined;
      }
      offset += length;
    }
    return undefined;
  }
  if (bytes.length >= 30 && matches(bytes, 0, "RIFF") && matches(bytes, 8, "WEBP") && view.getUint32(4, true) + 8 === fileSize) {
    const chunkSize = view.getUint32(16, true);
    if (20 + chunkSize + (chunkSize & 1) > fileSize) return undefined;
    if (matches(bytes, 12, "VP8X") && chunkSize === 10) {
      const width = 1 + bytes[24]! + (bytes[25]! << 8) + (bytes[26]! << 16);
      const height = 1 + bytes[27]! + (bytes[28]! << 8) + (bytes[29]! << 16);
      return { mimeType: "image/webp", width, height };
    }
    if (matches(bytes, 12, "VP8 ") && chunkSize >= 10 && (bytes[20]! & 1) === 0 && matches(bytes, 23, "\x9d\x01\x2a")) {
      const width = view.getUint16(26, true) & 0x3fff;
      const height = view.getUint16(28, true) & 0x3fff;
      if (width && height) return { mimeType: "image/webp", width, height };
    }
  }
  if (bytes.length >= 25 && matches(bytes, 0, "RIFF") && matches(bytes, 8, "WEBP") && matches(bytes, 12, "VP8L") && view.getUint32(4, true) + 8 === fileSize) {
    const chunkSize = view.getUint32(16, true);
    if (chunkSize >= 5 && 20 + chunkSize + (chunkSize & 1) <= fileSize && bytes[20] === 0x2f && (bytes[24]! >> 5) === 0) {
      const width = 1 + bytes[21]! + ((bytes[22]! & 0x3f) << 8);
      const height = 1 + (bytes[22]! >> 6) + (bytes[23]! << 2) + ((bytes[24]! & 0x0f) << 10);
      return { mimeType: "image/webp", width, height };
    }
  }
  return undefined;
}

/** Decode only the EBML header's bounded integer fields, not its media payload. */
function ebmlInteger(bytes: Uint8Array, offset: number, id: boolean): { value: number; next: number } | undefined {
  const first = bytes[offset];
  if (first === undefined || first === 0) return undefined;
  let marker = 0x80;
  let length = 1;
  while ((first & marker) === 0) { marker >>= 1; length++; }
  if (length > (id ? 4 : 8) || offset + length > bytes.length) return undefined;
  let value = id ? first : first & (marker - 1);
  for (let i = 1; i < length; i++) value = value * 256 + bytes[offset + i]!;
  // Unknown-sized elements are not valid inside an EBML header.
  if (!Number.isSafeInteger(value) || (!id && value === 2 ** (7 * length) - 1)) return undefined;
  return { value, next: offset + length };
}

function videoMime(bytes: Uint8Array, fileSize: number): string | undefined {
  if (bytes.length >= 16 && matches(bytes, 4, "ftyp")) {
    const size = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0);
    if (size < 16 || size > bytes.length || size > fileSize || size % 4 !== 0) return undefined;
    if (matches(bytes, 8, "qt  ")) return "video/quicktime";
    for (const brand of MP4_BRANDS) {
      if (matches(bytes, 8, brand)) return "video/mp4";
    }
    return undefined;
  }
  if (!matches(bytes, 0, "\x1a\x45\xdf\xa3")) return undefined;
  const header = ebmlInteger(bytes, 4, false);
  if (!header || header.value > bytes.length - header.next) return undefined;
  const end = header.next + header.value;
  if (!matches(bytes, end, "\x18\x53\x80\x67")) return undefined;
  let offset = header.next;
  let webm = false;
  while (offset < end) {
    const id = ebmlInteger(bytes, offset, true);
    if (!id || id.next > end) return undefined;
    const size = ebmlInteger(bytes, id.next, false);
    if (!size || size.next > end || size.value > end - size.next) return undefined;
    if (id.value === 0x4282) {
      if (webm || size.value !== 4 || !matches(bytes, size.next, "webm")) return undefined;
      webm = true;
    }
    offset = size.next + size.value;
  }
  return webm ? "video/webm" : undefined;
}

/** Derive metadata from the durable Blob's bytes, not the original name or File.type. */
export async function inspectNativeChatFileMetadata(blob: Blob, signal?: AbortSignal): Promise<AttachmentMetadata> {
  if (!Number.isInteger(blob.size) || blob.size < 0 || blob.size > U32_MAX) throw new Error("Chat file size exceeds the supported range");
  const fallback: AttachmentMetadata = { mimeType: "application/octet-stream", sizeBytes: blob.size, kind: { tag: "File" } };
  if (signal?.aborted) return fallback;
  const bytes = new Uint8Array(await blob.slice(0, HEADER_LIMIT).arrayBuffer());
  if (signal?.aborted) return fallback;
  const image = imageHeader(bytes, blob.size);
  if (image) {
    return { mimeType: image.mimeType, sizeBytes: blob.size, kind: { tag: "Image", value: { width: image.width, height: image.height, thumbnail: undefined } } };
  }
  const mimeType = videoMime(bytes, blob.size);
  if (!mimeType || typeof document === "undefined") return fallback;
  // Only a whitelisted media container reaches the browser decoder. The element
  // is detached, muted, metadata-only and never played or presented to a Guest.
  let video: HTMLVideoElement;
  let url: string;
  try {
    video = document.createElement("video");
    video.preload = "metadata";
    video.autoplay = false;
    video.muted = true;
    video.playsInline = true;
    url = URL.createObjectURL(blob.slice(0, blob.size, mimeType));
  } catch {
    return fallback;
  }
  return new Promise<AttachmentMetadata>((resolve) => {
    let settled = false;
    const finish = (metadata: AttachmentMetadata) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      video.onloadedmetadata = video.onerror = null;
      try {
        video.removeAttribute("src");
        video.load();
      } catch {
        // Cleanup failure must not retain the URL or strand the Host selection.
      } finally {
        URL.revokeObjectURL(url);
        resolve(metadata);
      }
    };
    const abort = () => finish(fallback);
    const timer = setTimeout(abort, VIDEO_TIMEOUT_MS);
    signal?.addEventListener("abort", abort, { once: true });
    video.onerror = abort;
    video.onloadedmetadata = () => {
      const duration = video.duration;
      if (!Number.isFinite(duration) || duration < 0 || duration > U32_MAX || video.videoWidth <= 0 || video.videoHeight <= 0) {
        finish(fallback);
      } else {
        finish({ mimeType, sizeBytes: blob.size, kind: { tag: "Video", value: { durationSeconds: Math.floor(duration), thumbnail: undefined } } });
      }
    };
    if (signal?.aborted) { abort(); return; }
    try { video.src = url; video.load(); } catch { abort(); }
  });
}

const IMAGE_EXTENSIONS: Readonly<Record<string, string>> = {
  "image/png": "png", "image/jpeg": "jpg", "image/gif": "gif", "image/webp": "webp",
};
const VIDEO_EXTENSIONS: Readonly<Record<string, string>> = {
  "video/mp4": "mp4", "video/quicktime": "mov", "video/webm": "webm",
};

/** Fixed safe basename and a kind-consistent media whitelist, never a supplied path. */
export function nativeChatExportFilename(metadata: AttachmentMetadata): string {
  let extension = "bin";
  if (metadata.kind.tag === "Image") {
    const { width, height } = metadata.kind.value;
    if (Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0 && width <= U32_MAX && height <= U32_MAX && Object.hasOwn(IMAGE_EXTENSIONS, metadata.mimeType)) {
      extension = IMAGE_EXTENSIONS[metadata.mimeType]!;
    }
  } else if (metadata.kind.tag === "Video") {
    const { durationSeconds } = metadata.kind.value;
    if (Number.isInteger(durationSeconds) && durationSeconds >= 0 && durationSeconds <= U32_MAX && Object.hasOwn(VIDEO_EXTENSIONS, metadata.mimeType)) {
      extension = VIDEO_EXTENSIONS[metadata.mimeType]!;
    }
  }
  return `chat-attachment.${extension}`;
}
