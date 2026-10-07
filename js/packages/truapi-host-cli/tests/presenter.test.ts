// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The prompt-routing contract. Inside embedded contexts (a git remote
// helper) the standard streams belong to someone else, so prompts must reach
// the controlling terminal instead, and a host that cannot ask must deny.

import { describe, it, expect } from "bun:test";
import { PassThrough } from "node:stream";
import { createTerminalPresenter } from "../src/presenter.js";
import type { TtyStreams } from "../src/tty.js";

const REQUEST = {
  title: "Sign a message",
  details: ["account: test.dot (derivation #0)"],
  phoneVerifies: true,
};

/** A writable that records everything written to it. */
function sink() {
  const chunks: string[] = [];
  const stream = new PassThrough();
  stream.on("data", (chunk: Buffer) => chunks.push(String(chunk)));
  return {
    stream: stream as unknown as NodeJS.WriteStream,
    text: () => chunks.join(""),
  };
}

/** A fake controlling terminal whose keyboard we can type on. */
function fakeTty() {
  const keyboard = new PassThrough();
  const screen = sink();
  let closed = false;
  const streams: TtyStreams = {
    input: keyboard as unknown as NodeJS.ReadStream,
    output: screen.stream,
    close() {
      closed = true;
    },
  };
  return {
    streams,
    type: (text: string) => keyboard.write(text),
    screenText: screen.text,
    wasClosed: () => closed,
  };
}

describe("createTerminalPresenter prompt routing", () => {
  it("As a git remote helper, I prompt on the controlling terminal and the approval flows back", async () => {
    // Given
    const out = sink();
    const tty = fakeTty();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => tty.streams,
    });

    // When
    const decision = presenter.confirm(REQUEST);
    tty.type("y\n");

    // Then
    expect(await decision).toBe(true);
    // The prompt block renders on the presenter output (stderr in real use);
    // the question and answer go through the terminal.
    expect(out.text()).toContain("Sign a message");
    expect(tty.screenText()).toContain("Continue? [y/N]");
    expect(tty.wasClosed()).toBe(true);
    presenter.dispose();
  });

  it("As a git remote helper, a non-approval answer on the terminal denies", async () => {
    // Given
    const out = sink();
    const tty = fakeTty();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => tty.streams,
    });

    // When
    const decision = presenter.confirm(REQUEST);
    tty.type("\n");

    // Then
    expect(await decision).toBe(false);
    expect(tty.wasClosed()).toBe(true);
    presenter.dispose();
  });

  it("As a process with no controlling terminal, prompts deny automatically", async () => {
    // Given
    const out = sink();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => undefined,
    });

    // When
    const decision = await presenter.confirm(REQUEST);

    // Then
    expect(decision).toBe(false);
    expect(out.text()).toContain("No controlling terminal");
    presenter.dispose();
  });

  it("As a user approving a batch, each prompt tells me how many approvals wait behind it", async () => {
    // Given
    const out = sink();
    // openTty is called once per prompt, so hand out a fresh terminal whose
    // keyboard answers by itself.
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => {
        const tty = fakeTty();
        setTimeout(() => tty.type("y\n"), 5);
        return tty.streams;
      },
    });

    // When
    const decisions = [
      presenter.confirm(REQUEST),
      presenter.confirm(REQUEST),
      presenter.confirm(REQUEST),
    ];

    // Then
    expect(await Promise.all(decisions)).toEqual([true, true, true]);
    expect(out.text()).toContain("(2 more approvals waiting behind this one)");
    expect(out.text()).toContain("(1 more approval waiting behind this one)");
    presenter.dispose();
  });

  it("As a batch operator with defaultYes, a bare Enter approves and the label says so", async () => {
    // Given
    const out = sink();
    const tty = fakeTty();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => tty.streams,
      defaultYes: true,
    });

    // When
    const decision = presenter.confirm(REQUEST);
    tty.type("\n");

    // Then
    expect(await decision).toBe(true);
    expect(tty.screenText()).toContain("[Y/n]");
    presenter.dispose();
  });

  it("As a batch operator with defaultYes, an explicit n still denies", async () => {
    // Given
    const out = sink();
    const tty = fakeTty();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => tty.streams,
      defaultYes: true,
    });

    // When
    const decision = presenter.confirm(REQUEST);
    tty.type("n\n");

    // Then
    expect(await decision).toBe(false);
    presenter.dispose();
  });

  it("As a batch operator with defaultYes on a non-TTY, prompts still deny", async () => {
    // Given
    const out = sink();
    const pipedStdin = new PassThrough() as unknown as NodeJS.ReadStream;
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: pipedStdin,
      defaultYes: true,
    });

    // When
    const decision = await presenter.confirm(REQUEST);

    // Then
    expect(decision).toBe(false);
    expect(out.text()).toContain("No interactive terminal");
    presenter.dispose();
  });

  it("As a CLI user, a note containing a long URL renders in full instead of being truncated", async () => {
    // Given
    const url = `https://example.com/${"a".repeat(90)}`;
    const out = sink();
    const tty = fakeTty();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => tty.streams,
    });

    // When
    const decision = presenter.confirm({
      title: "Publish data to the Bulletin chain",
      details: [],
      phoneVerifies: false,
      phoneNote: `See ${url} for details`,
    });
    tty.type("\n");
    await decision;

    // Then
    // Rejoin the wrapped lines. Every character of the URL must survive.
    const rendered = out
      .text()
      .split("\n")
      .map((line) => line.trim())
      .join(" ")
      .replace(/ /g, "");
    expect(rendered).toContain(url.replace(/ /g, ""));
    presenter.dispose();
  });

  it("As a CLI user granting a permission, I can allow it once, allow it always, or refuse", async () => {
    // Given
    const answers = ["o\n", "a\n", "nope\n"];
    const out = sink();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      openTty: () => {
        const tty = fakeTty();
        setTimeout(() => tty.type(answers.shift() ?? "\n"), 5);
        return tty.streams;
      },
    });

    // When
    const decisions = [
      await presenter.confirmPermission?.(REQUEST),
      await presenter.confirmPermission?.(REQUEST),
      await presenter.confirmPermission?.(REQUEST),
    ];

    // Then
    expect(decisions).toEqual(["AllowOnce", "AllowAlways", "Deny"]);
    presenter.dispose();
  });

  it("As a CLI user, a bare Enter never grants a permission for good", async () => {
    // Given
    // defaultYes exists so an operator can hold Enter through a batch. A
    // lasting authority grant must still be asked for explicitly.
    const out = sink();
    const withDefault = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      defaultYes: true,
      openTty: () => {
        const tty = fakeTty();
        setTimeout(() => tty.type("\n"), 5);
        return tty.streams;
      },
    });
    const withoutDefault = createTerminalPresenter({
      output: sink().stream,
      input: "tty",
      openTty: () => {
        const tty = fakeTty();
        setTimeout(() => tty.type("\n"), 5);
        return tty.streams;
      },
    });

    // Then
    expect(await withDefault.confirmPermission?.(REQUEST)).toBe("AllowOnce");
    expect(await withoutDefault.confirmPermission?.(REQUEST)).toBe("Deny");
    withDefault.dispose();
    withoutDefault.dispose();
  });

  it("As a process with no controlling terminal, a permission prompt denies", async () => {
    // Given
    const out = sink();
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: "tty",
      defaultYes: true,
      openTty: () => undefined,
    });

    // Then
    expect(await presenter.confirmPermission?.(REQUEST)).toBe("Deny");
    presenter.dispose();
  });

  it("As a CI process with piped stdin, prompts deny automatically", async () => {
    // Given
    const out = sink();
    const pipedStdin = new PassThrough() as unknown as NodeJS.ReadStream;
    const presenter = createTerminalPresenter({
      output: out.stream,
      input: pipedStdin,
    });

    // When
    const decision = await presenter.confirm(REQUEST);

    // Then
    expect(decision).toBe(false);
    expect(out.text()).toContain("No interactive terminal");
    presenter.dispose();
  });
});
