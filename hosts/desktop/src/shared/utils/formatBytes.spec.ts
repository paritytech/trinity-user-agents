import { describe, expect, it } from 'vitest';

import { formatBytes } from './formatBytes';

describe('formatBytes', () => {
  it('formats bytes', () => {
    expect(formatBytes(512)).toBe('512 B');
  });
  it('formats kilobytes with one decimal', () => {
    expect(formatBytes(1536)).toBe('1.5 KB');
  });
  it('formats megabytes with one decimal', () => {
    expect(formatBytes(15_000_000)).toBe('14.3 MB');
  });
  it('formats zero', () => {
    expect(formatBytes(0)).toBe('0 B');
  });
});
