import { describe, expect, it } from "bun:test";
import { settle } from "../test-support.js";
import {
  inspectNativeChatFileMetadata,
  nativeChatExportFilename,
} from "./native-chat-media.js";

// Header fixtures exercise metadata parsing, not an image renderer/decoder.
const png = new Uint8Array(33);
png.set([0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82]);
new DataView(png.buffer).setUint32(16, 640);
new DataView(png.buffer).setUint32(20, 480);
png.set([8, 6, 0, 0, 0], 24);
const gif = new Uint8Array([71, 73, 70, 56, 57, 97, 64, 1, 240, 0, 0, 0, 0]);
// The APP1 payload deliberately contains a fake SOF; it must be skipped by length.
const jpeg = new Uint8Array([
  0xff, 0xd8, 0xff, 0xe1, 0, 13, 0xff, 0xc0, 0, 11, 8, 0, 1, 0, 1, 1, 1, 0xff,
  0xc2, 0, 11, 8, 1, 44, 2, 88, 1, 1, 0x11, 0,
]);

function webp(chunk: string, payload: number[]): Uint8Array {
  const bytes = new Uint8Array(20 + payload.length + (payload.length & 1));
  const view = new DataView(bytes.buffer);
  bytes.set(new TextEncoder().encode("RIFF"));
  view.setUint32(4, bytes.length - 8, true);
  bytes.set(new TextEncoder().encode(`WEBP${chunk}`), 8);
  view.setUint32(16, payload.length, true);
  bytes.set(payload, 20);
  return bytes;
}

const mp4 = new Uint8Array([
  0, 0, 0, 24, 102, 116, 121, 112, 105, 115, 111, 109, 0, 0, 0, 0, 105, 115,
  111, 109, 109, 112, 52, 50,
]);
const webm = new Uint8Array([
  0x1a, 0x45, 0xdf, 0xa3, 0x87, 0x42, 0x82, 0x84, 119, 101, 98, 109, 0x18, 0x53,
  0x80, 0x67, 0xff,
]);

class MetadataVideo {
  src = "";
  preload = "";
  autoplay = false;
  muted = false;
  playsInline = false;
  duration = 12.9;
  videoWidth = 640;
  videoHeight = 480;
  onloadedmetadata: (() => void) | null = null;
  onerror: (() => void) | null = null;
  removeAttribute(name: string) {
    if (name === "src") this.src = "";
  }
  load() {}
  play(): never {
    throw new Error("Metadata inspection must never play media");
  }
}

async function withVideoDocument(
  run: (videos: MetadataVideo[]) => Promise<void>,
): Promise<void> {
  const original = Object.getOwnPropertyDescriptor(globalThis, "document");
  const videos: MetadataVideo[] = [];
  Object.defineProperty(globalThis, "document", {
    configurable: true,
    value: {
      createElement(name: string) {
        if (name !== "video")
          throw new Error("Only detached video metadata inspection is allowed");
        const video = new MetadataVideo();
        videos.push(video);
        return video;
      },
    },
  });
  try {
    await run(videos);
  } finally {
    if (original) Object.defineProperty(globalThis, "document", original);
    else Reflect.deleteProperty(globalThis, "document");
  }
}

describe("native Chat immutable media metadata", () => {
  for (const fixture of [
    {
      label: "PNG",
      bytes: png,
      mime: "image/png",
      width: 640,
      height: 480,
      extension: "png",
    },
    {
      label: "GIF",
      bytes: gif,
      mime: "image/gif",
      width: 320,
      height: 240,
      extension: "gif",
    },
    {
      label: "JPEG with APP1",
      bytes: jpeg,
      mime: "image/jpeg",
      width: 600,
      height: 300,
      extension: "jpg",
    },
    {
      label: "extended WebP",
      bytes: webp("VP8X", [0, 0, 0, 0, 0x3f, 1, 0, 0xef, 0, 0]),
      mime: "image/webp",
      width: 320,
      height: 240,
      extension: "webp",
    },
    {
      label: "lossy WebP",
      bytes: webp("VP8 ", [0, 0, 0, 0x9d, 1, 0x2a, 0x40, 1, 0xf0, 0]),
      mime: "image/webp",
      width: 320,
      height: 240,
      extension: "webp",
    },
    {
      label: "lossless WebP",
      bytes: webp("VP8L", [0x2f, 0x3f, 0xc1, 0x3b, 0]),
      mime: "image/webp",
      width: 320,
      height: 240,
      extension: "webp",
    },
  ]) {
    it(`extracts ${fixture.label} dimensions from bytes, ignoring the MIME label`, async () => {
      const metadata = await inspectNativeChatFileMetadata(
        new Blob([fixture.bytes], { type: "text/html" }),
      );
      expect(metadata).toEqual({
        mimeType: fixture.mime,
        sizeBytes: fixture.bytes.length,
        kind: {
          tag: "Image",
          value: {
            width: fixture.width,
            height: fixture.height,
            thumbnail: undefined,
          },
        },
      });
      expect(nativeChatExportFilename(metadata)).toBe(
        `chat-attachment.${fixture.extension}`,
      );
      for (let length = 0; length < fixture.bytes.length; length++) {
        const truncated = await inspectNativeChatFileMetadata(
          new Blob([fixture.bytes.slice(0, length)]),
        );
        expect(truncated.kind.tag).toBe("File");
      }
    });
  }

  it("does not load active content or MIME-spoofed files into a media element", async () => {
    await withVideoDocument(async (videos) => {
      for (const content of [
        "<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'/>",
        "<!doctype html><script>alert(1)</script>",
      ]) {
        const metadata = await inspectNativeChatFileMetadata(
          new Blob([content], { type: "video/mp4" }),
        );
        expect(metadata.kind.tag).toBe("File");
        expect(metadata.mimeType).toBe("application/octet-stream");
        expect(nativeChatExportFilename(metadata)).toBe("chat-attachment.bin");
      }
      expect(videos).toEqual([]);
    });
  });

  it("falls back for a JPEG whose dimensions are beyond the bounded header", async () => {
    const bytes = new Uint8Array(65_552);
    bytes.set([0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff]);
    bytes.set([0xff, 0xc0, 0, 11, 8, 0, 10, 0, 10, 1, 1, 0x11, 0], 65_539);
    expect(
      (await inspectNativeChatFileMetadata(new Blob([bytes]))).kind.tag,
    ).toBe("File");
  });

  for (const fixture of [
    { bytes: mp4, mime: "video/mp4", extension: "mp4" },
    { bytes: webm, mime: "video/webm", extension: "webm" },
  ]) {
    it(`probes whitelisted ${fixture.mime} bytes without playback and revokes its URL`, async () => {
      await withVideoDocument(async (videos) => {
        const pending = inspectNativeChatFileMetadata(
          new Blob([fixture.bytes], { type: "text/html" }),
        );
        await settle();
        expect(videos).toHaveLength(1);
        const video = videos[0]!;
        const url = video.src;
        expect((await fetch(url)).ok).toBe(true);
        expect(video.autoplay).toBe(false);
        video.onloadedmetadata!();
        const metadata = await pending;
        expect(metadata).toEqual({
          mimeType: fixture.mime,
          sizeBytes: fixture.bytes.length,
          kind: {
            tag: "Video",
            value: { durationSeconds: 12, thumbnail: undefined },
          },
        });
        expect(nativeChatExportFilename(metadata)).toBe(
          `chat-attachment.${fixture.extension}`,
        );
        await expect(fetch(url)).rejects.toThrow();
      });
    });
  }

  it("aborts a pending video probe, releases its URL and ignores a late event", async () => {
    await withVideoDocument(async (videos) => {
      const abort = new AbortController();
      const pending = inspectNativeChatFileMetadata(
        new Blob([mp4]),
        abort.signal,
      );
      await settle();
      const video = videos[0]!;
      const url = video.src;
      const late = video.onloadedmetadata!;
      abort.abort();
      expect((await pending).kind.tag).toBe("File");
      late();
      await expect(fetch(url)).rejects.toThrow();
    });
  });

  it("rejects nonfinite duration and audio-only MP4 as video attachments", async () => {
    await withVideoDocument(async (videos) => {
      const invalidDuration = inspectNativeChatFileMetadata(new Blob([mp4]));
      await settle();
      videos[0]!.duration = Infinity;
      videos[0]!.onloadedmetadata!();
      expect((await invalidDuration).kind.tag).toBe("File");
      const audio = inspectNativeChatFileMetadata(new Blob([mp4]));
      await settle();
      videos[1]!.videoWidth = 0;
      videos[1]!.onloadedmetadata!();
      expect((await audio).kind.tag).toBe("File");
    });
  });

  it("never promotes active MIME types or inconsistent kinds into executable extensions", () => {
    const image = {
      tag: "Image" as const,
      value: { width: 1, height: 1, thumbnail: undefined },
    };
    for (const mimeType of [
      "image/svg+xml",
      "text/html",
      "application/javascript",
      "__proto__",
      "image/png/../../x.html",
    ]) {
      expect(
        nativeChatExportFilename({ mimeType, sizeBytes: 1, kind: image }),
      ).toBe("chat-attachment.bin");
    }
    expect(
      nativeChatExportFilename({
        mimeType: "image/png",
        sizeBytes: 1,
        kind: { tag: "File" },
      }),
    ).toBe("chat-attachment.bin");
    expect(
      nativeChatExportFilename({
        mimeType: "image/png",
        sizeBytes: 1,
        kind: {
          tag: "Image",
          value: { width: 0, height: 1, thumbnail: undefined },
        },
      }),
    ).toBe("chat-attachment.bin");
  });
});
