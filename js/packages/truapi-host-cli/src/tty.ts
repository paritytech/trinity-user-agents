// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Access to the controlling terminal, for prompting when the standard
// streams belong to someone else (a git remote helper, a pipeline). A
// process's stdin is just whatever its parent wired to descriptor 0. Opening
// /dev/tty reaches the user's real terminal regardless of that wiring, which
// is the same convention ssh, sudo and git itself use for their prompts.

import { closeSync, openSync } from "node:fs";
import { ReadStream, WriteStream } from "node:tty";

/** A controlling-terminal lease: real keyboard in, real screen out. */
export interface TtyStreams {
  input: NodeJS.ReadStream;
  output: NodeJS.WriteStream;
  /** Release the terminal (closes both descriptors). */
  close(): void;
}

/**
 * Open the process's controlling terminal for one prompt.
 *
 * Returns `undefined` when there is none: CI, daemons, GUI-spawned
 * processes, or platforms without `/dev/tty`. Callers must treat that as
 * "cannot ask", which for a host means deny.
 */
export function openControllingTerminal(): TtyStreams | undefined {
  let readFd: number | undefined;
  let writeFd: number | undefined;
  try {
    readFd = openSync("/dev/tty", "r");
    writeFd = openSync("/dev/tty", "w");
    const input = new ReadStream(readFd);
    const output = new WriteStream(writeFd);
    return {
      input,
      output,
      close() {
        // Destroying the streams closes their descriptors.
        input.destroy();
        output.destroy();
      },
    };
  } catch {
    if (readFd !== undefined) {
      try {
        closeSync(readFd);
      } catch {
        // Already closed. Nothing to release.
      }
    }
    if (writeFd !== undefined) {
      try {
        closeSync(writeFd);
      } catch {
        // Already closed. Nothing to release.
      }
    }
    return undefined;
  }
}
