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

  test("each catalogue fits the RFC bound of 32 commands with unique names", () => {
    for (const catalogue of ["v2", "additions"] as const) {
      const specs = commandSpecs(catalogue);
      expect(specs.length).toBeLessThanOrEqual(32);
      expect(new Set(specs.map((spec) => spec.name)).size).toBe(specs.length);
      // The user changes the catalogue from either one.
      expect(specs.map((spec) => spec.name)).toEqual(expect.arrayContaining(["help", "commands"]));
    }
    const v2 = commandSpecs();
    expect(v2.find((spec) => spec.name === "help")?.argumentHint).toBeUndefined();
    expect(v2.find((spec) => spec.name === "edit")?.argumentHint).toBe("<id> <text>");
    expect(commandSpecs("additions").find((spec) => spec.name === "caps")).toBeDefined();
  });

  test("every command is in a catalogue", () => {
    const declared = new Set([...commandSpecs("v2"), ...commandSpecs("additions")].map((spec) => spec.name));
    expect([...declared].sort()).toEqual(COMMANDS.map((c) => c.name).sort());
  });

  test("help lists every command in bold", () => {
    const help = helpMarkdown();
    for (const { name } of COMMANDS) expect(help).toContain(`**/${name}**`);
    expect(help).toContain("`<id> <text>`");
  });

  test("search and history are the only source-only commands", () => {
    expect(COMMANDS.filter((c) => c.rooms !== "both").map((c) => c.name)).toEqual(["search", "history"]);
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
