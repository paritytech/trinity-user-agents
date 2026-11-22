import { test as base, expect, type Page, type TestInfo } from "@playwright/test";
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

type CandidateCounts = {
  total: number;
  byType: Record<string, number>;
  byMid: Record<string, number>;
};

// Snapshot only the initial call, after native gathering completes and before
// renegotiation or teardown. Each observed Description/IceCandidate maps to one
// private StatementStore message; the fixture does not publish to that service.
async function reportInitialCallSignaling(page: Page, testInfo: TestInfo) {
  await expect.poll(() => page.evaluate<string[]>(
    "mediaFixture.nativeIceSnapshot().map(peer => peer.gatheringState)",
  ), { timeout: 15_000 }).toEqual(["complete", "complete"]);
  const native = await page.evaluate<{
    bundlePolicy: string;
    rtcpMuxPolicy: string;
    transceiverMids: string[];
    dtlsTransportCount: number;
    iceTransportCount: number;
    candidates: CandidateCounts;
  }[]>("mediaFixture.nativeIceSnapshot()");
  const candidates = await page.evaluate<{
    signaled: CandidateCounts;
    descriptions: CandidateCounts[];
  }[]>("mediaFixture.iceCounts()");
  const signaling = await page.evaluate<{
    peers: { peer: number; Description: number; IceCandidate: number; total: number }[];
    totals: { Description: number; IceCandidate: number; total: number };
    privateStatementStoreMessagesForSignaling: number;
  }>("mediaFixture.callSignalingCounts()");
  await testInfo.attach("initial-call-host-signaling", {
    body: JSON.stringify({ phase: "initial-call-gathering-complete", signaling, candidates, native }, null, 2),
    contentType: "application/json",
  });
  expect(native).toHaveLength(2);
  expect(candidates).toHaveLength(2);
  expect(signaling.peers).toHaveLength(2);
  for (const peer of native) {
    expect(peer.bundlePolicy).toBe("max-bundle");
    expect(peer.rtcpMuxPolicy).toBe("require");
    expect(peer.transceiverMids).toEqual(["0", "1", "2"]);
    expect(peer.dtlsTransportCount).toBe(1);
    expect(peer.iceTransportCount).toBe(1);
    // Three audio/camera/screen m-lines must gather only for the bundle's mid,
    // not emit a separate copy of every candidate for each transceiver.
    expect(Object.keys(peer.candidates.byMid)).toEqual(["0"]);
    expect(peer.candidates.total).toBeGreaterThan(0);
  }
  for (const [index, peer] of signaling.peers.entries()) {
    expect(peer.Description).toBeGreaterThan(0);
    expect(peer.IceCandidate).toBe(candidates[index].signaled.total);
    expect(peer.IceCandidate).toBeGreaterThan(0);
    expect(Object.keys(candidates[index].signaled.byMid)).toEqual(["0"]);
    expect(peer.total).toBe(peer.Description + peer.IceCandidate);
  }
  expect(signaling.totals.IceCandidate).toBe(
    native.reduce((sum, peer) => sum + peer.candidates.total, 0),
  );
  expect(signaling.totals.Description).toBe(
    signaling.peers.reduce((sum, peer) => sum + peer.Description, 0),
  );
  expect(signaling.privateStatementStoreMessagesForSignaling).toBe(
    signaling.totals.Description + signaling.totals.IceCandidate,
  );
}

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
}, testInfo) => {
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
  await reportInitialCallSignaling(page, testInfo);
});

test("three transceivers share one ICE transport without tripling candidates when TURN is available", async ({
  page, turnServers,
}, testInfo) => {
  await page.evaluate((servers) => {
    Reflect.get(window, "mediaFixture").configureIceServers(servers);
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
  await reportInitialCallSignaling(page, testInfo);
  const counts = await page.evaluate<{ signaled: CandidateCounts }[]>("mediaFixture.iceCounts()");
  // A selected direct route can finish gathering before a TURN allocation arrives.
  for (const peer of counts) {
    expect(peer.signaled.byType.host).toBeGreaterThan(0);
  }
});

test("peers connect directly without TURN and host signaling preserves host candidates", async ({
  page,
}, testInfo) => {
  await page.evaluate("mediaFixture.startPairWithEarlyPicture()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  await reportInitialCallSignaling(page, testInfo);
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
  const counts = await page.evaluate<{ signaled: { byType: Record<string, number> } }[]>(
    "mediaFixture.iceCounts()",
  );
  await testInfo.attach("host-ice-candidate-counts", {
    body: JSON.stringify(counts, null, 2),
    contentType: "application/json",
  });
  expect(counts).toHaveLength(2);
  for (const peer of counts) expect(peer.signaled.byType.host).toBeGreaterThan(0);
});

test("rejected optional TURN credentials still allow a direct call", async ({ page }, testInfo) => {
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
  await reportInitialCallSignaling(page, testInfo);
});

test("hanging optional TURN credentials finish each create-peer within the five-second core budget", async ({
  page,
}, testInfo) => {
  await page.evaluate("mediaFixture.hangTurnCredentials()");
  await page.evaluate("mediaFixture.startPair()");
  await expect
    .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 15_000 })
    .toEqual({
      failures: [],
      captureRequests: 1,
      peers: ["Connected", "Connected"],
      remoteCameras: ["Off", "Live"],
    });
  const createPeerTimes = await page.evaluate<number[]>("mediaFixture.createPeerTimes()");
  expect(createPeerTimes).toHaveLength(2);
  for (const elapsed of createPeerTimes) {
    expect(elapsed).toBeGreaterThan(0);
    expect(elapsed).toBeLessThan(5_000);
  }
  // Connection includes two sequential credential waits plus ICE negotiation;
  // only each individual command is subject to the core's five-second budget.
  const connectionTimes = await page.evaluate<number[]>("mediaFixture.connectionTimes()");
  expect(connectionTimes).toHaveLength(2);
  for (const elapsed of connectionTimes) {
    expect(elapsed).toBeGreaterThan(0);
    expect(elapsed).toBeLessThan(15_000);
  }
  expect(await page.evaluate("mediaFixture.iceSnapshot().signaled")).toEqual(["host"]);
  await testInfo.attach("hanging-credentials-connection-times", {
    body: JSON.stringify({ createPeerTimes, connectionTimes }),
    contentType: "application/json",
  });
  await reportInitialCallSignaling(page, testInfo);
});

test("slow TURN discovery leaves the first peer direct and supplies the next peer", async ({
  page, turnServers,
}) => {
  await page.evaluate((servers) => {
    Reflect.get(window, "mediaFixture").slowTurnCredentials(servers);
  }, turnServers);
  await page.evaluate("mediaFixture.startPair()");
  await expect.poll(() => page.evaluate("mediaFixture.snapshot().peers"), {
    timeout: 15_000,
  }).toEqual(["Connected", "Connected"]);
  expect(await page.evaluate("mediaFixture.nativeIceSnapshot().map(peer => peer.iceServerCount)"))
    .toEqual([0, turnServers.length]);
  const createPeerTimes = await page.evaluate<number[]>("mediaFixture.createPeerTimes()");
  expect(createPeerTimes).toHaveLength(2);
  expect(createPeerTimes[0]).toBeGreaterThanOrEqual(1_900);
  for (const elapsed of createPeerTimes) expect(elapsed).toBeLessThan(5_000);
});

const malformedIceConfigurations: { name: string; iceServers: unknown }[] = [
  { name: "non-array ICE configuration", iceServers: { urls: "turn:127.0.0.1:3478" } },
  { name: "invalid TURN URL", iceServers: [{ urls: "turn:", username: "test", credential: "test" }] },
  { name: "TURN entry without credentials", iceServers: [{ urls: "turn:127.0.0.1:3478" }] },
];

for (const { name, iceServers } of malformedIceConfigurations) {
  test(`${name} rejected by native RTC still allows a direct call`, async ({ page }, testInfo) => {
    const constructorFailure = await page.evaluate((servers) => {
      Reflect.get(window, "mediaFixture").configureIceServers(servers);
      // Exercise Chromium's real WebIDL/RTC validation, not a constructor mock.
      try {
        const peer = new RTCPeerConnection({ iceServers: servers as RTCIceServer[] });
        peer.close();
        return null;
      } catch (error) {
        return error instanceof Error ? error.name : String(error);
      }
    }, iceServers);
    expect(constructorFailure).not.toBeNull();
    await page.evaluate("mediaFixture.startPairWithEarlyPicture()");
    await expect
      .poll(() => page.evaluate("mediaFixture.snapshot()"), { timeout: 5_000 })
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
    await reportInitialCallSignaling(page, testInfo);
  });

  test(`trusted relay-only policy does not bypass ${name}`, async ({ page }) => {
    await page.evaluate((servers) => {
      Reflect.get(window, "mediaFixture").configureIceServers(servers, "relay");
    }, iceServers);
    expect(await page.evaluate(async () => {
      try {
        await Reflect.get(window, "mediaFixture").startPair();
        return "Done";
      } catch {
        return "Rejected";
      }
    })).toBe("Rejected");
    await expect
      .poll(() => page.evaluate("mediaFixture.snapshot().peers[1]"))
      .toBe("Failed");
    expect(await page.evaluate("mediaFixture.iceSnapshot().signaled")).toEqual([]);
  });
}

for (const provider of ["rejectTurnCredentials", "hangTurnCredentials"]) {
  test(`trusted relay-only policy fails closed with ${provider}`, async ({ page }) => {
    await page.evaluate((method) => {
      Reflect.get(window, "mediaFixture")[method]("relay");
    }, provider);
    expect(await page.evaluate(async () => {
      try {
        await Reflect.get(window, "mediaFixture").startPair();
        return "Done";
      } catch {
        return "Rejected";
      }
    })).toBe("Rejected");
    await expect
      .poll(() => page.evaluate("mediaFixture.snapshot().peers[1]"))
      .toBe("Failed");
    expect(await page.evaluate("mediaFixture.iceSnapshot().signaled")).toEqual([]);
  });
}

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
