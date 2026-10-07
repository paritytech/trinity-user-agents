// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The shim's contract, measured against the core (tier 8): an operation's
// events must never reach the consumer before the start-response that names
// the operation. papi drops early events silently and the read never
// settles.

import { describe, it, expect } from "bun:test";
import {
  serializeOperationStarts,
  type JsonRpcProvider,
} from "../src/operation-order.js";

const startResponse = (id: string, operationId: string) => ({
  jsonrpc: "2.0",
  id,
  result: { result: "started", operationId },
});

const operationEvent = (event: string, operationId: string) => ({
  jsonrpc: "2.0",
  method: "chainHead_v1_followEvent",
  params: { subscription: "f1", result: { event, operationId } },
});

const followEvent = (event: string) => ({
  jsonrpc: "2.0",
  method: "chainHead_v1_followEvent",
  params: { subscription: "f1", result: { event } },
});

/** A provider we can push messages through, recording what the consumer saw. */
function harness() {
  let push: (message: unknown) => void = () => {};
  const provider: JsonRpcProvider = (onMessage) => {
    push = onMessage;
    return { send: () => {}, disconnect: () => {} };
  };
  const seen: unknown[] = [];
  serializeOperationStarts(provider)((message) => seen.push(message));
  return { push: (message: unknown) => push(message), seen };
}

const eventNames = (seen: unknown[]): string[] =>
  seen.map((message) => {
    const m = message as {
      id?: string;
      params?: { result?: { event?: string } };
    };
    return m.params?.result?.event ?? `response:${m.id ?? "?"}`;
  });

describe("serializeOperationStarts", () => {
  it("As a papi consumer, already-ordered traffic reaches me untouched", () => {
    // Given
    const { push, seen } = harness();

    // When
    push(followEvent("initialized"));
    push(startResponse("r1", "1"));
    push(operationEvent("operationStorageItems", "1"));
    push(operationEvent("operationStorageDone", "1"));

    // Then
    expect(eventNames(seen)).toEqual([
      "initialized",
      "response:r1",
      "operationStorageItems",
      "operationStorageDone",
    ]);
  });

  it("As a papi consumer, I never receive an operation event before the start-response that names it", () => {
    // Given
    const { push, seen } = harness();
    push(followEvent("initialized"));
    // The measured inversion: items and done arrive BEFORE the response that
    // names operationId 0.
    push(operationEvent("operationStorageItems", "0"));
    push(operationEvent("operationStorageDone", "0"));
    expect(eventNames(seen)).toEqual(["initialized"]);

    // When
    push(startResponse("r1", "0"));

    // Then
    expect(eventNames(seen)).toEqual([
      "initialized",
      "response:r1",
      "operationStorageItems",
      "operationStorageDone",
    ]);
  });

  it("As a papi consumer, interleaved operations are gated independently", () => {
    // Given
    const { push, seen } = harness();

    // When
    push(startResponse("r1", "1"));
    push(operationEvent("operationStorageItems", "2")); // held: not announced
    push(operationEvent("operationStorageItems", "1")); // flows: announced
    push(startResponse("r2", "2")); // releases the held event

    // Then
    expect(eventNames(seen)).toEqual([
      "response:r1",
      "operationStorageItems",
      "response:r2",
      "operationStorageItems",
    ]);
  });

  it("As a papi consumer, a start-response from a stopped follow never releases the refollow's events", () => {
    // Given
    const { push, seen } = harness();
    push(followEvent("stop"));
    // In flight from the dead follow. papi registered no subscriber for it.
    push(startResponse("r1", "1"));
    // The refollow reuses small operation numbers. Its "1" must NOT be
    // un-gated by the dead follow's start-response.
    push(operationEvent("operationStorageItems", "1"));
    expect(eventNames(seen)).toEqual(["stop", "response:r1"]);

    // When
    // The refollow comes alive. Its own start-response releases the event.
    push(followEvent("initialized"));
    push(startResponse("r2", "1"));

    // Then
    expect(eventNames(seen)).toEqual([
      "stop",
      "response:r1",
      "initialized",
      "response:r2",
      "operationStorageItems",
    ]);
  });

  it("As a papi consumer, a reused operation id is gated again after its terminal event", () => {
    // Given
    const { push, seen } = harness();
    push(startResponse("r1", "1"));
    push(operationEvent("operationStorageDone", "1"));
    // A NEW operation reusing the id must wait for its own start-response.
    push(operationEvent("operationStorageItems", "1"));
    expect(eventNames(seen)).toEqual(["response:r1", "operationStorageDone"]);

    // When
    push(startResponse("r2", "1"));

    // Then
    expect(eventNames(seen)).toEqual([
      "response:r1",
      "operationStorageDone",
      "response:r2",
      "operationStorageItems",
    ]);
  });
});
