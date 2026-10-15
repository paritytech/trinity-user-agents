import assert from "node:assert/strict";
import test from "node:test";

import { chatIdHex, decodeSqliteHexRows, logcatResumeTime } from "./android-emulator.mjs";

test("the chat id hex is the uppercase UTF-8 encoding of that string", () => {
  const hex = chatIdHex("truapi-playground.dot", "truapi-playground");
  assert.equal(hex, hex.toUpperCase());
  assert.equal(
    Buffer.from(hex, "hex").toString("utf8"),
    "ChatExtension:truapi-playground.dot:truapi-playground",
  );
  assert.equal(hex.slice(0, 28), "43686174457874656E73696F6E3A");
});

test("a room id keeps its own colon separator", () => {
  assert.equal(
    Buffer.from(chatIdHex("a.dot", "room:with:colons"), "hex").toString("utf8"),
    "ChatExtension:a.dot:room:with:colons",
  );
});

test("hex rows decode back to multi-line markdown", () => {
  const report = "## Truapi Chat Diagnosis\n\n**3 success · 0 failed**";
  const output = `${Buffer.from(report, "utf8").toString("hex")}\n`;
  assert.deepEqual(decodeSqliteHexRows(output), [report]);
});

test("non-hex noise in the sqlite output is dropped", () => {
  assert.deepEqual(decodeSqliteHexRows("Error: no such table\n4869\n"), ["Hi"]);
});

test("a logcat stream connects again from the time of its last threadtime line", () => {
  assert.equal(
    logcatResumeTime("10-15 02:23:11.193   456   513 I netd    : destroyNetworkCache(105) <0.40ms>"),
    "10-15 02:23:11.193",
  );
  assert.equal(logcatResumeTime("--------- beginning of main"), undefined);
  assert.equal(logcatResumeTime(undefined), undefined);
});
