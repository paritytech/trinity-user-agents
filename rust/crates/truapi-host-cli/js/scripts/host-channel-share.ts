/// <reference path="../runner.ts" />
// Host-channel demo, Alice's side: keep this signing host alive for its pairing,
// then, when the trigger file appears, disclose a profile reference to every
// Chat app's contacts and stay alive so the host channel can post and receive.
// Scratch script for the prototype; not part of the battery.
import { existsSync, unlinkSync } from "node:fs";
export {};
const TRIGGER = process.env.HOST_CHANNEL_TRIGGER ?? "/tmp/host-channel-share.go";
const STOP = process.env.HOST_CHANNEL_STOP ?? "/tmp/host-channel-share.stop";
const REFERENCE = process.env.HOST_CHANNEL_REFERENCE ?? "seity-contacts:v1:demo";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || (login.value !== "Success" && login.value !== "AlreadyConnected")) {
  throw new Error(`requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}
const user = await truapi.account.getUserId();
console.log("alice user:", user.isOk() ? JSON.stringify(user.value) : JSON.stringify(user.error));
console.log(`waiting for ${TRIGGER} (handshake first), then disclosing; ${STOP} ends the run`);
while (!existsSync(TRIGGER) && !existsSync(STOP)) await sleep(2000);
if (existsSync(TRIGGER)) {
  unlinkSync(TRIGGER);
  const result = await truapi.profile.disclose({ reference: REFERENCE, audiences: [{ tag: "ChatApps" }] });
  console.log("disclose:", JSON.stringify(result));
  console.log("HOST_CHANNEL_DISCLOSED");
}
while (!existsSync(STOP)) await sleep(2000);
unlinkSync(STOP);
console.log("HOST_CHANNEL_SHARE_DONE");
