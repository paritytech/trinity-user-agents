/// <reference path="../runner.ts" />
// Does the host grant the per-index Statement Store allowance a Chat device
// needs? echat asks for exactly this on its first request. Scratch script for
// the host-channel prototype; not part of the battery.
export {};
const login = await truapi.account.requestLogin({ reason: undefined });
if (!login.isOk() || (login.value !== "Success" && login.value !== "AlreadyConnected")) {
  throw new Error(`requestLogin failed: ${login.isOk() ? login.value : JSON.stringify(login.error)}`);
}
const user = await truapi.account.getUserId();
console.log("user:", user.isOk() ? JSON.stringify(user.value) : JSON.stringify(user.error));
const started = Date.now();
const result = await truapi.resourceAllocation.request({
  resources: [{ tag: "ProductStatementStoreAllowance", value: { tag: "Raw", value: "0x8390f327c2e6785b30da233de87fc95733b60f3b9194275232df45af2104d887" } }],
});
console.log(`allocation answered after ${((Date.now() - started) / 1000).toFixed(1)}s:`, JSON.stringify(result));
assert(result.isOk(), "resource allocation failed:", result);
const [outcome] = result.value.outcomes;
console.log("ProductStatementStoreAllowance(Raw) outcome:", outcome);
// Ask again: a second request should find the slot already allocated and answer fast.
const again = Date.now();
const second = await truapi.resourceAllocation.request({
  resources: [{ tag: "ProductStatementStoreAllowance", value: { tag: "Raw", value: "0x8390f327c2e6785b30da233de87fc95733b60f3b9194275232df45af2104d887" } }],
});
console.log(`second request after ${((Date.now() - again) / 1000).toFixed(1)}s:`, JSON.stringify(second));
console.log(outcome === "Allocated" ? "CHAT_ALLOWANCE_OK" : "CHAT_ALLOWANCE_NOT_ALLOCATED");
