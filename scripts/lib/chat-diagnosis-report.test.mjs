import assert from "node:assert/strict";
import test from "node:test";
import {
  REGISTER_BOT_GAP,
  decodeTextMessage,
  diagnosisFailures,
  labelChatDiagnosisReport,
} from "./chat-diagnosis-report.mjs";

const GREEN = [
  "## Truapi Chat Diagnosis",
  "",
  "**5 success · 0 failed**",
  "",
  "| Method | Status | Details |",
  "| --- | --- | --- |",
  "| `Chat/create_room` | ✅ | created |",
].join("\n");

const WITH_GAP = [
  "## Truapi Chat Diagnosis",
  "",
  "**5 success · 1 failed**",
  "",
  "| Method | Status | Details |",
  "| --- | --- | --- |",
  "| `Chat/create_room` | ✅ | created |",
  "| `Chat/register_bot` | ❌ | registerBot failed: this host has no bot registry |",
].join("\n");

test("decodes the multi-byte compact length used by a Chat report", () => {
  const text = `## Truapi Chat Diagnosis\n${"result ".repeat(20)}`;
  const body = Buffer.from(text);
  const compact = Buffer.alloc(2);
  compact.writeUInt16LE((body.length << 2) | 1);
  const encoded = Buffer.concat([Buffer.of(0), compact, body]);

  assert.equal(decodeTextMessage(encoded.toString("hex")), text);
  assert.equal(decodeTextMessage(Buffer.of(252).toString("hex")), undefined);
});

test("lists only the failed method rows", () => {
  assert.deepEqual(diagnosisFailures(WITH_GAP), [
    {
      method: "Chat/register_bot",
      details: "registerBot failed: this host has no bot registry",
    },
  ]);
  assert.deepEqual(diagnosisFailures(GREEN), []);
});

test("labels an all-green report and leaves its body alone", () => {
  assert.equal(
    labelChatDiagnosisReport(GREEN, "iOS"),
    GREEN.replace("## Truapi Chat", "## Truapi iOS Chat"),
  );
  assert.throws(() =>
    labelChatDiagnosisReport(GREEN.replace("5 success", "0 success"), "iOS"),
  );
  assert.throws(() =>
    labelChatDiagnosisReport(GREEN.replace("0 failed", "1 failed"), "iOS"),
  );
});

test("an accepted failure passes and is footnoted", () => {
  const labelled = labelChatDiagnosisReport(WITH_GAP, "Android", {
    acceptedFailures: REGISTER_BOT_GAP,
  });
  assert.match(labelled, /^## Truapi Android Chat Diagnosis/);
  assert.match(
    labelled,
    /\n\n_Accepted host gaps: Chat\/register_bot: registerBot failed: this host has no bot registry_$/,
  );
});

test("an unexpected failure still fails, accepted list or not", () => {
  assert.throws(() => labelChatDiagnosisReport(WITH_GAP, "Android"));
  const other = WITH_GAP.replace("Chat/register_bot", "Chat/post_message");
  assert.throws(() =>
    labelChatDiagnosisReport(other, "Android", {
      acceptedFailures: REGISTER_BOT_GAP,
    }),
  );
  const otherDetails = WITH_GAP.replace("no bot registry", "timed out");
  assert.throws(() =>
    labelChatDiagnosisReport(otherDetails, "Android", {
      acceptedFailures: REGISTER_BOT_GAP,
    }),
  );
});

test("a ❌ outside the table throws even when the summary says 0 failed", () => {
  const stray = `${GREEN}\n\n_the bridge dropped a call ❌_`;
  assert.throws(() => labelChatDiagnosisReport(stray, "Android"));
  assert.throws(() =>
    labelChatDiagnosisReport(stray, "Android", {
      acceptedFailures: REGISTER_BOT_GAP,
    }),
  );
});
