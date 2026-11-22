import { test as base, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { startTurnServer } from "./turn-server";

const test = base.extend<{ turnServers: RTCIceServer[] }>({
  turnServers: async ({}, use) => {
    const turn = await startTurnServer();
    try {
      await use(turn.iceServers);
    } finally {
      await turn.stop();
    }
  },
});

const core = new URL("../../../", import.meta.url);
const resources = new Map([
  [
    "/backend.mjs",
    new URL("js/packages/truapi-host/dist/web/browser-media-backend.js", core),
  ],
  [
    "/neverthrow.mjs",
    new URL("node_modules/neverthrow/dist/index.es.js", core),
  ],
  ["/fixture.mjs", new URL("./backend-fixture.mjs", import.meta.url)],
]);
const html = `<!doctype html><div id="controls"></div>
<script type="importmap">{"imports":{"neverthrow":"/neverthrow.mjs"}}</script>
<script type="module" src="/fixture.mjs"></script>`;

// Routes serve real compiled backend code. Native peers connect directly with
// no ICE servers unless a test explicitly requests the loopback TURN fixture.
test.beforeEach(async ({ page }) => {
  await page.route("http://localhost:48196/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const source = resources.get(path);
    await route.fulfill(
      source
        ? { contentType: "text/javascript", body: await readFile(source) }
        : { contentType: "text/html", body: html },
    );
  });
  await page.goto("http://localhost:48196/");
  await page.waitForFunction("!!window.mediaFixture");
});
test.afterEach(async ({ page }) => {
  await page.evaluate("window.mediaFixture?.dispose()");
});

test("a receive-only session opens without a screen capture API", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.disableScreenCapture()");
  expect(
    await page.evaluate("mediaFixture.openReceiveOnlyWithoutCapture()"),
  ).toEqual({
    supported: true,
    opened: "Done",
    committed: "LocalState",
    captureRequests: 0,
  });
});

test("camera calls and host video overlays work without a screen capture API", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.disableScreenCapture()");
  await page.evaluate("mediaFixture.startPairWithEarlyPicture()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(0);
});

test("a screen request fails when the browser has no screen capture API", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.disableScreenCapture()");
  expect(await page.evaluate("mediaFixture.openScreenWithoutCapture()")).toEqual({
    supported: true,
    result: {
      tag: "Rejected",
      value: {
        failure: {
          tag: "Domain",
          value: { error: { tag: "DeviceUnavailable" } },
        },
      },
    },
  });
});

test("trusted hosts can explicitly force the TURN fallback", async ({
  page, turnServers,
}) => {
  await page.evaluate((servers) => {
    Reflect.get(window, "mediaFixture").forceTurn(servers);
  }, turnServers);
  await page.evaluate("mediaFixture.startPair()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  expect(await page.evaluate("mediaFixture.iceSnapshot().signaled")).toEqual(["relay"]);
});

test("peers connect directly without TURN and host signaling preserves host candidates", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.startPairWithEarlyPicture()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  // Renegotiate after gathering so descriptions embed gathered candidates too.
  await page.evaluate("mediaFixture.setAnswererCamera(true)");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toMatchObject({ remoteCameras: ["Live", "Live"] });
  await expect
    .poll(() => page.evaluate("mediaFixture.iceSnapshot()"))
    .toEqual({ signaled: ["host"], descriptions: ["host"] });
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(0);
  // The real sandboxed product cannot read the trusted host's signaling,
  // peer statistics or compositor, even though host candidates now carry IPs.
  for (const frame of page.frames().slice(1)) {
    expect(await frame.evaluate(() => {
      let hostReadable = false;
      try {
        hostReadable = !!parent.document;
      } catch {}
      return {
        hostReadable,
        backend: typeof Reflect.get(window, "mediaFixture"),
        mediaElements: document.querySelectorAll("video, audio").length,
      };
    })).toEqual({ hostReadable: false, backend: "undefined", mediaElements: 0 });
  }
});

test("rejected optional TURN credentials still allow a direct call", async ({ page }) => {
  await page.evaluate("mediaFixture.rejectTurnCredentials()");
  await page.evaluate("mediaFixture.startPairWithEarlyPicture()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  expect(await page.evaluate("mediaFixture.iceSnapshot().signaled")).toEqual(["host"]);
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(0);
});

test("capture failures expose only a typed error, not engine network details", async ({ page }) => {
  expect(await page.evaluate("mediaFixture.rejectCapture()")).toEqual({
    tag: "Rejected",
    value: {
      failure: { tag: "Domain", value: { error: { tag: "DeviceUnavailable" } } },
    },
  });
});

test("an initially receive-only answerer connects and resumes camera after stopping", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.startPair()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  await page.evaluate("mediaFixture.setAnswererCamera(true)");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 2,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Live", "Live"],
    });
  await page.evaluate("mediaFixture.setAnswererCamera(false)");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 2,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  await page.evaluate("mediaFixture.setAnswererCamera(true)");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 3,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Live", "Live"],
    });
});

test("a picture placed for an admitted participant before its peer exists shows once media arrives", async ({
  page,
}) => {
  expect(
    await page.evaluate("mediaFixture.startPairWithEarlyPicture()"),
  ).toEqual({ tag: "Done" });
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(0);
});

test("a picture keeps its video element and follows the last accepted layout while the product scrolls", async ({
  page,
}) => {
  expect(
    await page.evaluate("mediaFixture.startPairWithEarlyPicture()"),
  ).toEqual({ tag: "Done" });
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"), {
      timeout: 15_000,
    })
    .toBeGreaterThan(0);
  expect(
    await page.evaluate("mediaFixture.scrollPicture({ frames: 20, step: 3 })"),
  ).toEqual({
    replaced: 0,
    kept: true,
    finalTop: 60,
    expectedTop: 60,
    stale: "StaleLayout",
  });
});

test("cancellation stops a genuine capture delivered after cancellation", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.prepareDelayedCapture()");
  await expect
    .poll(() => page.evaluate("mediaFixture.captureWaiting()"))
    .toBe(true);
  expect(await page.evaluate("mediaFixture.cancelDelayedCapture()")).toEqual({
    result: "Rejected",
    tracks: ["ended"],
  });
});

test("an event stream opened after attachment starts at the current viewport without replaying the detach", async ({
  page,
}) => {
  // Attachment detaches (revision 1, None) and then measures (revision 2).
  expect(await page.evaluate("mediaFixture.attachBeforeObserving()")).toEqual({
    failures: [],
    core: 2,
    viewports: [2],
    surfaces: "Done",
  });
});

test("synthetic-device camera withdrawal ends capture but preserves independent decoded receive video", async ({
  page,
}) => {
  await page.evaluate("mediaFixture.startCameraWithdrawal()");
  await expect
    .poll(() => page.evaluate("mediaFixture.withdrawalSnapshot()"), {
      timeout: 15_000,
    })
    .toEqual({
      failures: [],
      captureRequests: 2,
      peer: "Connected",
      camera: "Live",
      revocations: [[], []],
    });
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(0);
  await page.evaluate("mediaFixture.prepareDelayedAnswererCamera()");
  await expect
    .poll(() => page.evaluate("mediaFixture.captureWaiting()"))
    .toBe(true);
  const cancelled = {
    tag: "Rejected",
    value: {
      failure: {
        tag: "Domain",
        value: { error: { tag: "OperationCancelled" } },
      },
    },
  };
  expect(await page.evaluate("mediaFixture.withdrawCamera()")).toEqual({
    pending: cancelled,
    commit: cancelled,
    session: {
      tag: "Rejected",
      value: {
        failure: { tag: "Domain", value: { error: { tag: "InvalidHandle" } } },
      },
    },
    stoppedCapture: ["ended"],
    committed: "LocalState",
    lateCapture: ["ended"],
  });
  await expect
    .poll(() => page.evaluate("mediaFixture.withdrawalSnapshot()"))
    .toEqual({
      failures: [],
      captureRequests: 3,
      peer: "Connected",
      camera: "Live",
      revocations: [
        [{ permission: "Camera", source: "OperatingSystem" }],
        [{ permission: "Camera", source: "OperatingSystem" }],
      ],
    });
  // Observe new decoded frames after withdrawal, not a stale state event/image.
  const framesAfterWithdrawal = await page.evaluate<number>(
    "mediaFixture.decodedFrames()",
  );
  await expect
    .poll(() => page.evaluate("mediaFixture.decodedFrames()"))
    .toBeGreaterThan(framesAfterWithdrawal);
});
