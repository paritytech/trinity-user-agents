import { describe, expect, it } from 'vitest';

import { type HexString } from '@/shared/types';

import { type RootManifest } from './schemas';
import { manifestService } from './service';
import { type AppExecutable } from './types';

const TEST_HASH: HexString = '0xdeadbeef';

describe('parseRootManifest', () => {
  it('parses a valid v1 root manifest', () => {
    const raw = JSON.stringify({
      $v: 1,
      displayName: 'HackM3',
      description: 'a test product',
      icon: { cid: 'bafy123', format: 'png' },
    });
    expect(manifestService.parseRootManifest(raw)).toEqual({
      $v: 1,
      displayName: 'HackM3',
      description: 'a test product',
      icon: { cid: 'bafy123', format: 'png' },
    });
  });

  it('returns null for empty text record', () => {
    expect(manifestService.parseRootManifest('')).toBeNull();
  });

  it('returns null for malformed JSON', () => {
    expect(manifestService.parseRootManifest('not json')).toBeNull();
  });

  it('returns null for unknown $v', () => {
    const raw = JSON.stringify({
      $v: 2,
      displayName: 'HackM3',
      description: 'a',
      icon: { cid: 'bafy', format: 'png' },
    });
    expect(manifestService.parseRootManifest(raw)).toBeNull();
  });

  // An unknown `icon.format` degrades the icon to a placeholder but the product
  // stays launchable — so the root manifest must still parse, preserving the raw
  // format string for the render-time guard.
  it('parses root manifest with unknown icon.format (icon degrades to placeholder)', () => {
    const raw = JSON.stringify({
      $v: 1,
      displayName: 'HackM3',
      description: 'a',
      icon: { cid: 'bafy', format: 'svg' },
    });
    expect(manifestService.parseRootManifest(raw)).toEqual({
      $v: 1,
      displayName: 'HackM3',
      description: 'a',
      icon: { cid: 'bafy', format: 'svg' },
    });
  });

  it('returns null when missing required field', () => {
    const raw = JSON.stringify({
      $v: 1,
      displayName: 'HackM3',
      icon: { cid: 'bafy', format: 'png' },
    });
    expect(manifestService.parseRootManifest(raw)).toBeNull();
  });
});

describe('parseExecutableManifest', () => {
  it('parses a valid app manifest under app subname', () => {
    const raw = JSON.stringify({ $v: 1, kind: 'app', appVersion: [1, 0, 0] });
    expect(manifestService.parseExecutableManifest(raw, 'app')).toEqual({
      $v: 1,
      kind: 'app',
      appVersion: [1, 0, 0],
    });
  });

  it('accepts 4-element SemVer tuple with build identifier', () => {
    const raw = JSON.stringify({ $v: 1, kind: 'app', appVersion: [1, 0, 0, 'abc123'] });
    expect(manifestService.parseExecutableManifest(raw, 'app')).toEqual({
      $v: 1,
      kind: 'app',
      appVersion: [1, 0, 0, 'abc123'],
    });
  });

  it('parses a valid widget manifest', () => {
    const raw = JSON.stringify({
      $v: 1,
      kind: 'widget',
      appVersion: [1, 0, 0],
      dimensions: { height: [2, 4, 8] },
    });
    const parsed = manifestService.parseExecutableManifest(raw, 'widget');
    expect(parsed).toEqual({
      $v: 1,
      kind: 'widget',
      appVersion: [1, 0, 0],
      dimensions: { height: [2, 4, 8] },
    });
  });

  it('parses a valid worker manifest with chat only', () => {
    const raw = JSON.stringify({
      $v: 1,
      kind: 'worker',
      appVersion: [1, 0, 0],
      entrypoint: 'index.js',
      includes: { chat: true, pocket: false },
    });
    expect(manifestService.parseExecutableManifest(raw, 'worker')).toEqual({
      $v: 1,
      kind: 'worker',
      appVersion: [1, 0, 0],
      entrypoint: 'index.js',
      includes: { chat: true, pocket: false },
    });
  });

  // A worker with both `includes` false is a valid background-only worker
  // (no Pocket/Chat surface); it still launches, so it must parse.
  it('parses worker with both includes false (background-only worker)', () => {
    const raw = JSON.stringify({
      $v: 1,
      kind: 'worker',
      appVersion: [1, 0, 0],
      entrypoint: 'index.js',
      includes: { chat: false, pocket: false },
    });
    expect(manifestService.parseExecutableManifest(raw, 'worker')).toEqual({
      $v: 1,
      kind: 'worker',
      appVersion: [1, 0, 0],
      entrypoint: 'index.js',
      includes: { chat: false, pocket: false },
    });
  });

  it('rejects manifest whose kind does not match the subname (app under worker subname)', () => {
    const raw = JSON.stringify({ $v: 1, kind: 'app', appVersion: [1, 0, 0] });
    expect(manifestService.parseExecutableManifest(raw, 'worker')).toBeNull();
  });

  it('returns null for empty text record', () => {
    expect(manifestService.parseExecutableManifest('', 'app')).toBeNull();
  });

  it('returns null for malformed JSON', () => {
    expect(manifestService.parseExecutableManifest('not json', 'app')).toBeNull();
  });

  it('returns null for unknown kind', () => {
    const raw = JSON.stringify({ $v: 1, kind: 'mystery', appVersion: [1, 0, 0] });
    expect(manifestService.parseExecutableManifest(raw, 'app')).toBeNull();
  });
});

describe('isRenderableIconFormat', () => {
  it('accepts the v1 raster formats', () => {
    expect(manifestService.isRenderableIconFormat('png')).toBe(true);
    expect(manifestService.isRenderableIconFormat('jpeg')).toBe(true);
    expect(manifestService.isRenderableIconFormat('PNG')).toBe(true);
    expect(manifestService.isRenderableIconFormat('JPEG')).toBe(true);
  });

  it('rejects unknown formats so the icon falls back to a placeholder', () => {
    expect(manifestService.isRenderableIconFormat('svg')).toBe(false);
    expect(manifestService.isRenderableIconFormat('')).toBe(false);
  });
});

describe('legacyApp', () => {
  it('builds a zero-version app executable from just an identifier + contenthash', () => {
    expect(manifestService.legacyApp('app.dot', '0xabc')).toEqual({
      kind: 'app',
      identifier: 'app.dot',
      appVersion: [0, 0, 0],
      contenthash: '0xabc',
    });
  });
});

describe('assembleProduct', () => {
  it('combines root + executables + owner into a Product struct', () => {
    const root: RootManifest = {
      $v: 1,
      displayName: 'HackM3',
      description: 'a test product',
      icon: { cid: 'bafy123', format: 'png' },
    };
    const app: AppExecutable = { kind: 'app', identifier: 'hackm3.dot', appVersion: [1, 0, 0], contenthash: TEST_HASH };

    expect(
      manifestService.assembleProduct({
        baseName: 'hackm3.dot',
        root,
        executables: { app },
        owner: '0xabc',
      }),
    ).toEqual({
      baseName: 'hackm3.dot',
      displayName: 'HackM3',
      description: 'a test product',
      icon: { cid: 'bafy123', format: 'png' },
      executables: { app },
      owner: '0xabc',
    });
  });
});

describe('formatVersion', () => {
  it('formats a 3-tuple semver', () => {
    expect(manifestService.formatVersion([2, 1, 1])).toBe('2.1.1');
  });

  it('formats a 4-tuple semver (build id appended)', () => {
    expect(manifestService.formatVersion([2, 1, 1, 'abc123'])).toBe('2.1.1.abc123');
  });

  it('formats an all-zero legacy semver verbatim (omit rule lives in the feature, not here)', () => {
    expect(manifestService.formatVersion([0, 0, 0])).toBe('0.0.0');
  });
});

describe('sniffImageFormat', () => {
  const png = (...rest: number[]) => new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, ...rest]);
  const jpeg = (...rest: number[]) => new Uint8Array([0xff, 0xd8, 0xff, ...rest]);

  it('recognizes PNG and JPEG by their magic bytes', () => {
    expect(manifestService.sniffImageFormat(png(0, 1, 2))).toBe('png');
    expect(manifestService.sniffImageFormat(jpeg(0xe0, 0))).toBe('jpeg');
  });

  // The host draws only what it can name. A GIF is a real image and still refused —
  // the allowlist is the format pair the icon path already accepts, not "is it an image".
  it('refuses a format outside the allowlist', () => {
    const gif = new Uint8Array([0x47, 0x49, 0x46, 0x38, 0x39, 0x61]);

    expect(manifestService.sniffImageFormat(gif)).toBeNull();
  });

  // A truncated buffer must not read past its end and must not match on a prefix.
  it('refuses a buffer shorter than the magic it would match', () => {
    expect(manifestService.sniffImageFormat(new Uint8Array([0x89, 0x50, 0x4e]))).toBeNull();
    expect(manifestService.sniffImageFormat(new Uint8Array([0xff, 0xd8]))).toBeNull();
    expect(manifestService.sniffImageFormat(new Uint8Array())).toBeNull();
  });
});

describe('lookupArchiveFile', () => {
  const bytes = new Uint8Array([1]);

  it('matches a path stored under either slash convention', () => {
    expect(manifestService.lookupArchiveFile({ 'img/a.png': bytes }, '/img/a.png')).toBe(bytes);
    expect(manifestService.lookupArchiveFile({ '/img/a.png': bytes }, 'img/a.png')).toBe(bytes);
    expect(manifestService.lookupArchiveFile({ 'img/a.png': bytes }, 'img/a.png')).toBe(bytes);
  });

  it('is undefined for a path the archive does not carry', () => {
    expect(manifestService.lookupArchiveFile({ 'img/a.png': bytes }, 'img/b.png')).toBeUndefined();
  });
});

describe('executionKindOf', () => {
  it.each([
    ['app', 'App'],
    ['widget', 'Widget'],
    ['worker', 'Worker'],
  ] as const)('names a manifest %j executable %j, as the core does', (kind, executionKind) => {
    expect(manifestService.executionKindOf(kind)).toBe(executionKind);
  });
});
