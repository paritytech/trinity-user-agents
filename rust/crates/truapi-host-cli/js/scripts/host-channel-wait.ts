/// <reference path="../runner.ts" />
// Host-channel demo, Bob's side: keep this signing host alive for its pairing
// and its background receiver until the stop file appears. The host's own log
// shows `host channel key recorded for a peer` and
// `host channel reference recorded from a contact`.
import { existsSync, unlinkSync } from "node:fs";
export {};
const STOP = process.env.HOST_CHANNEL_STOP ?? "/tmp/host-channel-wait.stop";
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || (login.value !== "Success" && login.value !== "AlreadyConnected")) {
  throw new Error(`requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}
const user = await truapi.account.getUserId();
console.log("bob user:", user.isOk() ? JSON.stringify(user.value) : JSON.stringify(user.error));
console.log(`alive until ${STOP}`);
while (!existsSync(STOP)) await sleep(2000);
unlinkSync(STOP);
console.log("HOST_CHANNEL_WAIT_DONE");
