import { existsSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { type LogFile } from 'electron-log';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Electron's main-process API is a host boundary: it does not exist outside the Electron runtime.
vi.mock('electron', () => ({ app: {}, dialog: {}, ipcMain: {} }));

import { rotateLogs } from './logs';

function createLogFile(path: string): LogFile {
  return { path, bytesWritten: 0, size: 0, clear: vi.fn(() => true), on: vi.fn() };
}

describe('rotateLogs', () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'logs-spec-'));
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
    vi.restoreAllMocks();
  });

  it('archives the log under a name that is valid on Windows', () => {
    const path = join(dir, 'polkadot-desktop.log');
    writeFileSync(path, 'line');

    rotateLogs(createLogFile(path));

    expect(existsSync(path)).toBe(false);
    const [archive, ...rest] = readdirSync(dir);
    expect(rest).toEqual([]);
    expect(archive).toMatch(/^polkadot-desktop\..+\.log$/);
    expect(archive).not.toMatch(/[<>:"|?*]/);
  });

  it('keeps only the newest archives of this log and leaves other files alone', () => {
    const path = join(dir, 'polkadot-desktop.log');
    writeFileSync(path, 'line');
    writeFileSync(join(dir, 'unrelated.txt'), '');
    for (let day = 10; day < 22; day++) {
      writeFileSync(join(dir, `polkadot-desktop.2026-01-${day}T00-00-00.000Z.log`), '');
    }

    rotateLogs(createLogFile(path));

    const archives = readdirSync(dir).filter(file => file.startsWith('polkadot-desktop.'));
    expect(archives).toHaveLength(10);
    expect(archives).not.toContain('polkadot-desktop.2026-01-10T00-00-00.000Z.log');
    expect(archives).not.toContain('polkadot-desktop.2026-01-12T00-00-00.000Z.log');
    expect(archives).toContain('polkadot-desktop.2026-01-13T00-00-00.000Z.log');
    expect(readdirSync(dir)).toContain('unrelated.txt');
  });

  it('clears the log without logging through console when the rename fails', () => {
    // `console` is electron-log in the app: a warning through it re-enters the oversized file transport and
    // recurses into another rotation.
    const warn = vi.spyOn(console, 'warn');
    const file = createLogFile(join(dir, 'missing.log'));

    rotateLogs(file);

    expect(file.clear).toHaveBeenCalledOnce();
    expect(warn).not.toHaveBeenCalled();
  });
});
