#!/usr/bin/env bun

import type {
  TrUApiClient,
  HostContext,
  ScriptAssert,
} from "./__TRUAPI_TYPES__";

declare const truapi: TrUApiClient;
declare const host: HostContext;
declare const assert: ScriptAssert;

// Scripts can use packages installed next to the script or in a parent project.

const result = await truapi.account.getUserId();
if (!result.isOk()) {
  throw new Error(`getUserId failed: ${JSON.stringify(result.error)}`);
}

console.log("user id", result.value);
