import { describe, expect, test } from "bun:test";
import {
  COMMANDS,
  commandSpecs,
  findCommand,
  helpMarkdown,
  parseCommand,
  parseCommandEvent,
} from "../../worker/commands";
import { RUNNABLE_COMMANDS } from "../../worker/execute";
import { splitFallback, splitGraphemes } from "../../worker/graphemes";

describe("parseCommand", () => {
  test("splits the name, the words and the rest", () => {
    expect(parseCommand("  /Edit last   hello   world ")).toEqual({
      name: "edit",
      args: ["last", "hello", "world"],
      rest: "last   hello   world",
      line: "/edit last hello world",
    });
  });

  test("gives no command for plain text or a bare slash", () => {
    expect(parseCommand("hello /help")).toBeUndefined();
    expect(parseCommand("/")).toBeUndefined();
    expect(parseCommand("/ help")).toBeUndefined();
  });

  test("parses a command without arguments", () => {
    expect(parseCommand("/help")).toEqual({ name: "help", args: [], rest: "", line: "/help" });
  });

  test("keeps the inner spaces of a free text argument", () => {
    expect(parseCommand("/meta  My   room")?.rest).toBe("My   room");
  });

  test("parses a Command event with or without the slash", () => {
    expect(parseCommandEvent({ command: "react", payload: "last 👍" })?.line).toBe("/react last 👍");
    expect(parseCommandEvent({ command: "/history", payload: "40" })?.args).toEqual(["40"]);
    expect(parseCommandEvent({ command: "echo", payload: "" })?.line).toBe("/echo");
  });
});

describe("the command table", () => {
  test("has one runner per command and no other", () => {
    expect([...RUNNABLE_COMMANDS].sort()).toEqual(COMMANDS.map((c) => c.name).sort());
  });

  test("fits the RFC bound of 32 commands with unique names", () => {
    const specs = commandSpecs();
    expect(specs.length).toBeLessThanOrEqual(32);
    expect(new Set(specs.map((spec) => spec.name)).size).toBe(specs.length);
    expect(specs.find((spec) => spec.name === "help")?.argumentHint).toBeUndefined();
    expect(specs.find((spec) => spec.name === "edit")?.argumentHint).toBe("<id> <text>");
  });

  test("help lists every command in bold", () => {
    const help = helpMarkdown();
    for (const { name } of COMMANDS) expect(help).toContain(`**/${name}**`);
    expect(help).toContain("`<id> <text>`");
  });

  test("history is the only source-only command", () => {
    expect(COMMANDS.filter((c) => c.rooms !== "both").map((c) => c.name)).toEqual(["history"]);
    expect(findCommand("history")?.posts).toBe(true);
    expect(findCommand("meta")?.posts).toBe(false);
  });
});

describe("splitGraphemes", () => {
  test("gives one entry per emoji", () => {
    expect(splitGraphemes("👍❤️")).toEqual(["👍", "❤️"]);
    expect(splitGraphemes(" 👍 🎉 ")).toEqual(["👍", "🎉"]);
    expect(splitGraphemes("")).toEqual([]);
  });

  test("the fallback joins ZWJ sequences, skin tones and flags", () => {
    expect(splitFallback("👍❤️")).toEqual(["👍", "❤️"]);
    expect(splitFallback("👨‍👩‍👧👍🏽")).toEqual(["👨‍👩‍👧", "👍🏽"]);
    expect(splitFallback("🇫🇷🇩🇪")).toEqual(["🇫🇷", "🇩🇪"]);
    expect(splitFallback("1️⃣a")).toEqual(["1️⃣", "a"]);
  });
});
