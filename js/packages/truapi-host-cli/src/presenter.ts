// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The terminal presentation layer. A host IS the UI layer: the web host
// answers the core with modals, this one answers with a QR and readline
// prompts. Everything is swappable behind `HostPresenter` so an embedding CLI
// can restyle without re-wiring callbacks (and tests can script decisions).

import * as readline from "node:readline/promises";
import type { AuthState, PermissionDecision } from "@parity/truapi-host";
import { shortHex } from "./hex.js";
import { renderQrTerminal } from "./qr.js";
import type { ConfirmRequest } from "./reviews.js";
import { openControllingTerminal, type TtyStreams } from "./tty.js";

export interface HostPresenter {
  /** Render the core-owned auth lifecycle (QR, progress, session identity). */
  authStateChanged(state: AuthState): void;
  /** Ask the user to approve a reviewed action or permission. */
  confirm(request: ConfirmRequest): Promise<boolean>;
  /**
   * Ask the user to approve something whose consent has a LIFETIME: allow it
   * once, allow it from now on, or refuse. Permissions and identity
   * disclosures take this path.
   *
   * Optional. A presenter that answers only yes/no still works: the host
   * treats a yes as the lasting grant, which is what the pre-0.21 boolean
   * permission callbacks recorded.
   */
  confirmPermission?(request: ConfirmRequest): Promise<PermissionDecision>;
  /** Show a product notification. */
  notify(text: string): void;
  /** Hand a URL to the user (the CLI cannot assume a system browser). */
  openUrl(url: string): void;
  dispose(): void;
}

export interface TerminalPresenterOptions {
  /** Where to render. Defaults to stderr so piped stdout stays clean. */
  output?: NodeJS.WriteStream;
  /**
   * Where confirm prompts read from. Defaults to stdin.
   *
   * Pass `"tty"` when the standard streams belong to someone else, e.g.
   * inside a git remote helper where git owns stdin/stdout: each prompt then
   * opens the controlling terminal (`/dev/tty`) for the question and answer,
   * the way ssh, sudo and git itself prompt. Without a controlling terminal
   * (CI, daemons) prompts deny, exactly like a non-TTY stdin.
   */
  input?: NodeJS.ReadStream | "tty";
  /** Override the QR renderer (tests, exotic terminals). */
  renderQr?: (deeplink: string) => Promise<string>;
  /**
   * Override how `input: "tty"` reaches the controlling terminal. Exists for
   * tests and exotic platforms. Defaults to opening `/dev/tty`.
   */
  openTty?: () => TtyStreams | undefined;
  /**
   * Make an empty answer (just Enter) APPROVE. The prompt then shows `[Y/n]`.
   *
   * Off by default: confirm prompts are deny-by-default, so a stray Enter
   * never approves a signature or allocation. Enable it only for trusted,
   * high-volume flows (e.g. a dogfooding CLI where the operator wants to hold
   * Enter through a batch). A non-TTY still denies regardless.
   */
  defaultYes?: boolean;
}

export function createTerminalPresenter(
  options: TerminalPresenterOptions = {},
): HostPresenter {
  const output = options.output ?? process.stderr;
  const input = options.input ?? process.stdin;
  const openTty = options.openTty ?? openControllingTerminal;
  const renderQr = options.renderQr ?? renderQrTerminal;
  const defaultYes = options.defaultYes ?? false;
  const write = (text: string): void => {
    output.write(text);
  };
  const promptLabel = defaultYes ? "  Continue? [Y/n] " : "  Continue? [y/N] ";
  // An empty answer takes the default. Anything else approves only on an
  // explicit yes.
  const isApproval = (answer: string): boolean => {
    const trimmed = answer.trim();
    return trimmed === "" ? defaultYes : /^y(es)?$/i.test(trimmed);
  };
  // A lasting grant must be asked for explicitly, so a bare Enter under
  // `defaultYes` gives the narrowest approval rather than the broadest.
  const readPermissionAnswer = (answer: string): PermissionDecision => {
    const trimmed = answer.trim();
    if (trimmed === "") {
      return defaultYes ? "AllowOnce" : "Deny";
    }
    if (/^o(nce)?$/i.test(trimmed)) {
      return "AllowOnce";
    }
    if (/^(a(lways)?|y(es)?)$/i.test(trimmed)) {
      return "AllowAlways";
    }
    return "Deny";
  };

  // Word-wrap a note without ever dropping text: unbroken runs longer than
  // the width (URLs, hashes) are hard-chunked. A regex like /.{1,68}(\s|$)/
  // silently discards the head of such runs, which for a safety-relevant
  // note is the worst possible failure mode.
  const wrapNote = (text: string, width = 68): string[] => {
    const wrapped: string[] = [];
    let line = "";
    for (const word of text.split(/\s+/)) {
      const pieces = word.match(new RegExp(`.{1,${String(width)}}`, "g")) ?? [];
      for (const piece of pieces) {
        if (line === "") {
          line = piece;
        } else if (line.length + 1 + piece.length <= width) {
          line = `${line} ${piece}`;
        } else {
          wrapped.push(line);
          line = piece;
        }
      }
    }
    if (line !== "") {
      wrapped.push(line);
    }
    return wrapped;
  };

  let progressTimer: NodeJS.Timeout | null = null;
  let disposed = false;
  // Confirm prompts share one stdin. Serialize them so two overlapping
  // reviews can never interleave their answers.
  let promptChain: Promise<void> = Promise.resolve();
  // Confirms currently entered and not yet answered. Batch submissions fire
  // many identical reviews at once (bulk Bulletin writes are the primary
  // case). Telling the user how many are queued behind the current prompt is
  // the only batch context the host has.
  let pendingConfirms = 0;

  const clearProgress = (): void => {
    if (progressTimer !== null) {
      clearInterval(progressTimer);
      progressTimer = null;
      if (output.isTTY) {
        write("\n");
      }
    }
  };

  const startProgress = (label: string): void => {
    clearProgress();
    const startedAt = Date.now();
    if (!output.isTTY) {
      write(`${label}\n`);
      return;
    }
    write(label);
    progressTimer = setInterval(() => {
      const elapsed = Math.round((Date.now() - startedAt) / 1000);
      write(`\r${label} ${String(elapsed)}s`);
    }, 1000);
    // A progress line must never be the thing keeping the process alive.
    progressTimer.unref();
  };

  /**
   * Render one prompt and read one answer, whatever shape that answer takes.
   *
   * Prompts are serialized so queued reviews cannot interleave their
   * questions, and every path that cannot reach a human returns `refusal`:
   * a host that cannot ask must not approve.
   */
  function ask<T>(
    request: ConfirmRequest,
    label: string,
    interpret: (answer: string) => T,
    refusal: T,
  ): Promise<T> {
    pendingConfirms += 1;
    const decision = promptChain.then(async () => {
      if (disposed) {
        return refusal;
      }
      clearProgress();
      const lines = [
        "",
        `▸ ${request.title}`,
        ...request.details.map((detail) => `    ${detail}`),
      ];
      const waiting = pendingConfirms - 1;
      if (waiting > 0) {
        lines.push(
          `    (${String(waiting)} more approval${waiting === 1 ? "" : "s"} waiting behind this one)`,
        );
      }
      if (request.phoneNote !== undefined) {
        for (const line of wrapNote(request.phoneNote)) {
          lines.push(`    ${line}`);
        }
      } else if (request.phoneVerifies) {
        lines.push(
          "    Verify the full details in the Polkadot app on your phone.",
          "    Nothing is signed until you approve it there.",
        );
      }
      write(`${lines.join("\n")}\n`);
      if (input === "tty") {
        // The standard streams belong to someone else (a git remote
        // helper). Ask on the controlling terminal instead. Opened per
        // prompt so an idle host holds no terminal descriptors.
        const tty = openTty();
        if (tty === undefined) {
          write("  No controlling terminal, denying automatically.\n");
          return refusal;
        }
        const rl = readline.createInterface({
          input: tty.input,
          output: tty.output,
        });
        try {
          return interpret(await rl.question(label));
        } finally {
          rl.close();
          tty.close();
        }
      }
      if (!input.isTTY) {
        write("  No interactive terminal, denying automatically.\n");
        return refusal;
      }
      const rl = readline.createInterface({ input, output });
      try {
        return interpret(await rl.question(label));
      } finally {
        rl.close();
      }
    });
    // Decrement before the next queued prompt renders: `finally` is
    // registered ahead of the chain link below, so it settles first.
    const settled = decision.finally(() => {
      pendingConfirms -= 1;
    });
    promptChain = settled.then(
      () => {},
      () => {},
    );
    return decision;
  }

  return {
    authStateChanged(state) {
      if (disposed) {
        return;
      }
      switch (state.tag) {
        case "Pairing": {
          const { deeplink } = state.value;
          void renderQr(deeplink).then(
            (qr) => {
              if (disposed) {
                return;
              }
              write(
                `\nScan with the Polkadot app to sign in:\n\n${qr}\n` +
                  `Or open this link on your phone:\n  ${deeplink}\n\n`,
              );
            },
            () => {
              // A QR that fails to render must not hide the deeplink.
              write(
                `\nOpen this link on your phone to sign in:\n  ${deeplink}\n\n`,
              );
            },
          );
          break;
        }
        case "Authenticating":
          // The People-chain statement round-trip runs ~20s with no further
          // callback. Without this line the host looks hung on a stale QR.
          startProgress(
            "Confirmed on your phone. Completing sign-in (about 20 seconds)…",
          );
          break;
        case "Connected": {
          clearProgress();
          const { publicKey, fullUsername, liteUsername } = state.value;
          const username = fullUsername ?? liteUsername;
          write(
            `Signed in${username !== undefined ? ` as ${username}` : ""} (${shortHex(publicKey)}).\n`,
          );
          break;
        }
        case "LoginFailed":
          clearProgress();
          write(`Sign-in failed: ${state.value.reason}\n`);
          break;
        case "Disconnected":
          clearProgress();
          write("Signed out.\n");
          break;
      }
    },

    confirm(request) {
      return ask(request, promptLabel, isApproval, false);
    },

    confirmPermission(request) {
      // Consent with a lifetime. Deny stays the default on every
      // cannot-ask path, and a bare Enter never grants the lasting form.
      return ask(
        request,
        defaultYes
          ? "  Allow? [o]nce / [a]lways / [N]o (Enter: once) "
          : "  Allow? [o]nce / [a]lways / [N]o ",
        readPermissionAnswer,
        "Deny" as PermissionDecision,
      );
    },

    notify(text) {
      write(`• ${text}\n`);
    },

    openUrl(url) {
      write(`Open this link in your browser:\n  ${url}\n`);
    },

    dispose() {
      disposed = true;
      clearProgress();
    },
  };
}
