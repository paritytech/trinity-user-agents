import { describe, expect, test } from "bun:test";
import {
  LocalChatDemo,
  demoAllowed,
} from "../../../../tools/demos/local-demo.js";
import {
  DEMO_WORKER_PATH,
  type DemoWorkerAnswer,
} from "../../../../tools/demos/local-demo-route.js";

const ANSWER: DemoWorkerAnswer = {
  record: "this dev server",
  cid: "bafkdemo",
  kind: "car",
  manifest: JSON.stringify({
    $v: 1,
    kind: "worker",
    appVersion: [0, 1, 0],
    entrypoint: "index.js",
    includes: { chat: true },
  }),
};

function setup() {
  let changes = 0;
  const asked: string[] = [];
  const demo = new LocalChatDemo({
    base: "/host/",
    origin: "http://localhost:5180",
    port: "5180",
    changed: () => (changes += 1),
    fetch: (async (url: string) => {
      asked.push(url);
      return new Response(JSON.stringify(ANSWER));
    }) as typeof fetch,
  });
  return { demo, asked, changes: () => changes };
}

describe("the local chat product demo", () => {
  test("is offered only by a dev server on this machine", () => {
    expect(demoAllowed(true, "localhost")).toBe(true);
    expect(demoAllowed(true, "127.0.0.1")).toBe(true);
    expect(demoAllowed(false, "localhost")).toBe(false);
    expect(demoAllowed(true, "192.168.1.20")).toBe(false);
    expect(demoAllowed(true, "host.example")).toBe(false);
  });

  test("adds its worker only while on, under a development product id", () => {
    const { demo, changes } = setup();
    expect(demo.productId).toBe("localhost:5180");
    expect(demo.workerNames()).toEqual([]);
    expect(demo.resolves("localhost:5180")).toBe(false);
    demo.start();
    demo.start();
    expect(demo.workerNames()).toEqual(["localhost:5180"]);
    expect(demo.resolves("localhost:5180")).toBe(true);
    expect(demo.resolves("chat.paseo")).toBe(false);
    demo.stop();
    expect(demo.workerNames()).toEqual([]);
    expect(changes()).toBe(2);
  });

  test("resolves to the archive the dev server built, fetched from this origin", async () => {
    const { demo, asked } = setup();
    demo.start();
    const worker = await demo.resolve();
    expect(asked).toEqual([`/host/${DEMO_WORKER_PATH}`]);
    expect(worker).toMatchObject({
      cid: "bafkdemo",
      kind: "car",
      gateway: "http://localhost:5180",
      manifest: { entrypoint: "index.js", includes: { chat: true } },
    });
  });

  test("gives the synthetic call engine to its own product only, while on", () => {
    const { demo } = setup();
    expect(demo.media("localhost:5180")).toBeNull();
    demo.start();
    expect(demo.media("localhost:5180")?.name).toBe("synthetic");
    expect(demo.media("chat.paseo")).toBeNull();
    demo.stop();
    expect(demo.media("localhost:5180")).toBeNull();
  });
});
