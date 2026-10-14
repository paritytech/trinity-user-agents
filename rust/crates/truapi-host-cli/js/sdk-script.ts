#!/usr/bin/env bun
import { createApp } from "@parity/product-sdk";

const app = await createApp({
  name: "my-app",
  logLevel: "info",
});

// Connect to host-provided accounts.
const { accounts } = await app.wallet.connect();

console.log("Connected accounts:", accounts);

// Persist a value. Namespaced under the app name in host storage.
await app.localStorage.set("lastVisit", new Date().toISOString());

const lastVisit = await app.localStorage.get("lastVisit");
console.log("Last visit:", lastVisit);
