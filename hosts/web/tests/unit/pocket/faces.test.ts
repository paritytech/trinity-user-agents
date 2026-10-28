import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import type { RendererNode } from "@parity/truapi";
import type { TrUApiProductProvider } from "@parity/truapi-host";
import {
  CONNECT_RETRY_MS,
  CONNECT_WINDOW_MS,
  KEEP_INTERVAL_MS,
  MAX_REOPEN_MS,
  PocketFaces,
  REOPEN_MS,
  pocketFacesKey,
  type PocketWorkers,
} from "../../../src/pocket/faces.js";
import { MemoryStorage } from "../../support/test-support.js";

const CARD = { productId: "example.paseo", cardId: "membership" };
const SESSION = { walletId: "w1", networkId: "paseo", cards: [CARD] };

const face = (text: string): RendererNode => ({
  tag: "Text",
  value: {
    modifiers: [],
    props: {},
    children: [{ tag: "String", value: { text } }],
  },
});

interface Render {
  request: unknown;
  sink: {
    onUpdate(tree: RendererNode): void;
    onError?(error: Error): void;
    onComplete?(): void;
  };
  stopped: boolean;
}

/** A worker's channel to the core that records the renders and actions it is asked for. */
function fakeProvider() {
  const renders: Render[] = [];
  const actions: unknown[] = [];
  const provider = {
    render(request: unknown, sink: Render["sink"]) {
      const render = { request, sink, stopped: false };
      renders.push(render);
      return () => (render.stopped = true);
    },
    publishRendererAction(item: unknown) {
      actions.push(item);
      return Promise.resolve();
    },
  } as unknown as TrUApiProductProvider;
  return { provider, renders, actions };
}

/** Product workers in which the test decides what runs. */
function fakeWorkers() {
  const listeners = new Set<() => void>();
  const workers = {
    held: [] as [string, string[]][],
    running: null as TrUApiProductProvider | null,
    holdForPocket(name: string, cardIds: string[]) {
      workers.held.push([name, cardIds]);
      return Promise.resolve();
    },
    serves: () => workers.running !== null,
    provider: () => workers.running,
    onChange(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    run(provider: TrUApiProductProvider | null) {
      workers.running = provider;
      for (const listener of listeners) listener();
    },
  };
  return workers satisfies PocketWorkers;
}

let now = 0;
beforeEach(() => {
  now = 0;
  jest.useFakeTimers();
});
afterEach(() => jest.useRealTimers());

const advance = (ms: number) => {
  now += ms;
  jest.advanceTimersByTime(ms);
};

function setup(storage = new MemoryStorage()) {
  const workers = fakeWorkers();
  const logs: string[] = [];
  const faces = new PocketFaces({
    workers,
    storage,
    log: (text) => logs.push(text),
    now: () => now,
  });
  return { faces, workers, storage, logs };
}

describe("PocketFaces", () => {
  // Nothing runs for Pocket until the user has seen it in this session.
  test("starts the card's worker the first time Pocket is shown", () => {
    const { faces, workers } = setup();
    faces.setSession(SESSION);
    expect(workers.held).toEqual([]);
    faces.setShown(true);
    faces.setShown(false);
    faces.setShown(true);
    expect(workers.held).toEqual([["example.paseo", ["membership"]]]);
  });

  // The face is drawn by the worker on the card's own render context, and a
  // press goes back on the same context.
  test("draws the worker's faces and sends presses back on the card's context", async () => {
    const { faces, workers } = setup();
    const { provider, renders, actions } = fakeProvider();
    faces.setSession(SESSION);
    faces.setShown(true);
    expect(faces.face(CARD)).toEqual({ tree: null, source: "bundled" });
    workers.run(provider);
    expect(renders.map((render) => render.request)).toEqual([
      {
        context: { tag: "PocketCard", value: { cardId: "membership" } },
        payload: "0x",
      },
    ]);
    renders[0]!.sink.onUpdate(face("one"));
    renders[0]!.sink.onUpdate(face("two"));
    expect(faces.face(CARD)).toEqual({ tree: face("two"), source: "live" });

    faces.send(CARD, "open", new Uint8Array([1, 2]));
    await Promise.resolve();
    expect(actions).toEqual([
      {
        context: { tag: "PocketCard", value: { cardId: "membership" } },
        actionId: "open",
        payload: "0x0102",
      },
    ]);
  });

  // A worker that just started has not registered its renderer yet, so it is
  // asked again quickly; one that keeps failing is asked less and less often.
  test("asks a booting worker again at once, then backs off", () => {
    const { faces, workers } = setup();
    const { provider, renders } = fakeProvider();
    faces.setSession(SESSION);
    faces.setShown(true);
    workers.run(provider);
    renders[0]!.sink.onError!(new Error("no renderer yet"));
    expect(renders[0]!.stopped).toBe(true);
    advance(CONNECT_RETRY_MS);
    expect(renders.length).toBe(2);

    advance(CONNECT_WINDOW_MS);
    const delays: number[] = [];
    for (let attempt = 0; attempt < 7; attempt += 1) {
      const before = renders.length;
      renders.at(-1)!.sink.onComplete!();
      let waited = 0;
      while (renders.length === before) {
        advance(250);
        waited += 250;
      }
      delays.push(waited);
    }
    expect(delays).toEqual([
      REOPEN_MS,
      REOPEN_MS * 2,
      REOPEN_MS * 4,
      REOPEN_MS * 8,
      REOPEN_MS * 16,
      MAX_REOPEN_MS,
      MAX_REOPEN_MS,
    ]);

    // A face resets the backoff.
    renders.at(-1)!.sink.onUpdate(face("back"));
    const before = renders.length;
    renders.at(-1)!.sink.onError!(new Error("dropped"));
    advance(REOPEN_MS);
    expect(renders.length).toBe(before + 1);
  });

  // A worker that restarts is a new execution: its stream replaces the old one.
  test("reopens the face stream when the worker restarts, and closes it when it stops", () => {
    const { faces, workers } = setup();
    const first = fakeProvider();
    const second = fakeProvider();
    faces.setSession(SESSION);
    faces.setShown(true);
    workers.run(first.provider);
    first.renders[0]!.sink.onUpdate(face("first"));
    workers.run(second.provider);
    expect(first.renders[0]!.stopped).toBe(true);
    expect(second.renders.length).toBe(1);
    workers.run(null);
    expect(second.renders[0]!.stopped).toBe(true);
    // The card keeps the last face it was drawn with.
    expect(faces.face(CARD).tree).toEqual(face("first"));
    advance(MAX_REOPEN_MS);
    expect(second.renders.length).toBe(1);
  });

  // The newest face is shown at once next time, before the worker answers,
  // and only for the wallet and network that saw it.
  test("keeps the newest face for the session, at most every interval", () => {
    const storage = new MemoryStorage();
    const { faces, workers } = setup(storage);
    const { provider, renders } = fakeProvider();
    faces.setSession(SESSION);
    faces.setShown(true);
    workers.run(provider);
    const kept = () =>
      JSON.parse(storage.getItem(pocketFacesKey("paseo", "w1")) ?? "{}")[
        "example.paseo/membership"
      ];
    renders[0]!.sink.onUpdate(face("one"));
    expect(kept()).toEqual(face("one"));
    renders[0]!.sink.onUpdate(face("two"));
    expect(kept()).toEqual(face("one"));
    advance(KEEP_INTERVAL_MS);
    expect(kept()).toEqual(face("two"));
    renders[0]!.sink.onUpdate(face("three"));
    // Ending the session writes the last face down whatever the interval says.
    faces.setSession(null);
    expect(kept()).toEqual(face("three"));
    expect(renders[0]!.stopped).toBe(true);

    const again = setup(storage);
    again.faces.setSession(SESSION);
    expect(again.faces.face(CARD)).toEqual({
      tree: face("three"),
      source: "kept",
    });
    again.faces.setSession({ ...SESSION, walletId: "w2" });
    expect(again.faces.face(CARD).source).toBe("bundled");
    again.faces.setSession({ ...SESSION, networkId: "previewnet" });
    expect(again.faces.face(CARD).source).toBe("bundled");
  });

  test("draws but does not keep a face too large to store, and ignores a kept value that is not a tree", () => {
    const storage = new MemoryStorage();
    storage.setItem(
      pocketFacesKey("paseo", "w1"),
      JSON.stringify({ "example.paseo/membership": "not a tree" }),
    );
    const { faces, workers, logs } = setup(storage);
    const { provider, renders } = fakeProvider();
    faces.setSession(SESSION);
    expect(faces.face(CARD).source).toBe("bundled");
    faces.setShown(true);
    workers.run(provider);
    renders[0]!.sink.onUpdate(face("x".repeat(300 * 1024)));
    expect(faces.face(CARD).source).toBe("live");
    expect(logs.some((line) => line.includes("too large to keep"))).toBe(true);
  });

  test("does not deliver a press while the card's worker is not running", () => {
    const { faces, logs } = setup();
    faces.setSession(SESSION);
    faces.setShown(true);
    faces.send(CARD, "open", new Uint8Array());
    expect(logs.at(-1)).toContain("not delivered");
  });
});
