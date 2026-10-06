import { createBrowserMediaBackend } from "/backend.mjs";

const product = { productId: "media-regression.dot", executionKind: "App" };
const id = (value) => `0x${value.toString(16).padStart(64, "0")}`;
const off = { microphone: false, camera: false, screen: false };
// Synthetic-device qualification: capture and RTC stay native. Only delivery of
// a genuine getUserMedia result is gated to exercise cancellation deterministically.
const nativeCapture = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
const captures = [];
let delayNextCapture = false;
let releaseCapture;
navigator.mediaDevices.getUserMedia = async (constraints) => {
  const stream = await nativeCapture(constraints);
  captures.push(stream);
  if (delayNextCapture) {
    delayNextCapture = false;
    await new Promise((resolve) => { releaseCapture = resolve; });
  }
  return stream;
};
// Candidate types the native agent gathered, observed beside the backend's own
// listener. Only set by `ignoreRelayPolicy()`.
const gathered = new Set();
// Every candidate type that left a backend, in IceCandidate events or SDP.
const signaled = new Set();
const candidateType = (line) => / typ (\S+)/.exec(line)?.[1] ?? "unknown";

const nodes = [];
const failures = [];
async function command(node, tag, value) {
  const result = await node.backend.mediaBackendCommand(product, node.runtime, { tag, value });
  if (result.tag === "Rejected") throw new Error(`${tag}: ${JSON.stringify(result.value.failure)}`);
  return result;
}
function signal(node, source, event) {
  const participantId = id(source.index + 1);
  let channel = node.signals.get(participantId);
  if (!channel) {
    channel = { pending: Promise.resolve() };
    node.signals.set(participantId, channel);
  }
  channel.pending = channel.pending.then(() => command(node,
    event.tag === "Description" ? "ApplyDescription" : "AddIceCandidate",
    event.tag === "Description"
      ? { sessionId: node.session, participantId, description: event.value.description }
      : { sessionId: node.session, participantId, candidate: event.value.candidate },
  )).catch((error) => failures.push(error.message));
}
async function observe(node) {
  try {
    for await (const item of node.backend.mediaBackendEvents(product, node.runtime)) {
      if (item.isErr()) throw new Error("Backend observation failed");
      const event = item.value;
      if (event.tag === "Description" || event.tag === "IceCandidate") {
        if (event.tag === "IceCandidate") signaled.add(candidateType(event.value.candidate.candidate));
        else for (const line of event.value.description.sdp.split("\r\n"))
          if (line.startsWith("a=candidate:")) signaled.add(candidateType(line));
        signal(nodes[Number(BigInt(event.value.participantId)) - 1], node, event);
      } else if (event.tag === "PeerStateChanged") {
        node.peer = event.value.state;
      } else if (event.tag === "RemoteStateChanged") {
        node.remote = event.value.state;
      } else if (event.tag === "ViewportChanged") {
        node.viewport = event.value.viewport;
      } else if (event.tag === "PermissionRevoked") {
        node.revocations.push(event.value);
      }
    }
  } catch (error) {
    failures.push(error.message);
  }
}
function createNode(index) {
  const mount = document.createElement("section");
  mount.style.cssText = "position:relative;isolation:isolate;width:320px;height:200px";
  const frame = document.createElement("iframe");
  frame.sandbox = "allow-scripts";
  frame.allow = "camera 'none'; microphone 'none'; display-capture 'none'; fullscreen 'none'";
  frame.srcdoc = "Untrusted product";
  frame.style.cssText = "position:absolute;inset:0;z-index:1;width:100%;height:100%;border:0";
  mount.append(frame);
  document.body.append(mount);
  const node = { index, runtime: BigInt(index + 1), session: id(100 + index), intentRevision: 0n,
    mount, signals: new Map(), revocations: [] };
  node.backend = createBrowserMediaBackend({
    window, document, productId: product.productId,
    getProductElement: () => frame,
    getCompositorMount: () => mount,
    isProductIsolated: () => !frame.sandbox.contains("allow-same-origin"),
    indicatorMount: document.querySelector("#controls"),
    // Relay-only peers need the loopback TURN relay started by turn-server.ts.
    iceServers: window.mediaIceServers,
    requestConsent: async () => { throw new Error("Unexpected consent request"); },
  });
  return node;
}
function remoteCameraSurface(node, participantId) {
  return {
    sessionId: node.session, viewportRevision: node.viewport.revision, layoutRevision: 0n,
    surfaces: [{
      surfaceId: 1, source: { tag: "Remote", value: { participantId, picture: "Camera" } },
      rect: { x: 0, y: 0, width: 320, height: 200 }, clip: { x: 0, y: 0, width: 320, height: 200 },
      cornerRadius: 0, depth: 0, fit: "Contain", mirrored: false, visible: true, placement: "AboveProduct",
    }],
  };
}
async function openPair() {
  nodes.push(createNode(0), createNode(1));
  for (const node of nodes) {
    const capabilities = await node.backend.mediaBackendCapabilities(product);
    if (!capabilities.supported) throw new Error("Browser Media unsupported");
    node.observer = observe(node);
    node.backend.attach(node.runtime);
  }
  if (captures.length) throw new Error("Capability discovery captured media");
  for (const node of nodes) {
    const operationId = id(200 + node.index);
    await command(node, "OpenSession", { sessionId: node.session, operationId,
      tracks: { ...off, camera: node.index === 0 } });
    await command(node, "CommitOperation", { operationId });
  }
}
async function connectPair() {
  await command(nodes[1], "CreatePeer", { sessionId: nodes[1].session, participantId: id(1), offerer: false });
  await command(nodes[0], "CreatePeer", { sessionId: nodes[0].session, participantId: id(2), offerer: true });
}

window.mediaFixture = {
  async startPair() {
    await openPair();
    await connectPair();
  },
  async startPairWithEarlyPicture() {
    await openPair();
    const receiver = nodes[1];
    while (!receiver.viewport) await new Promise((resolve) => setTimeout(resolve, 10));
    // The core admits a participant before the call handshake creates its
    // backend peer; the product may already place that participant's picture.
    const early = await receiver.backend.mediaBackendCommand(product, receiver.runtime,
      { tag: "SetSurfaces", value: remoteCameraSurface(receiver, id(1)) });
    await connectPair();
    return early;
  },
  async setAnswererCamera(enabled) {
    const node = nodes[1];
    const operationId = id(300 + Number(++node.intentRevision));
    await command(node, "SetTracks", { sessionId: node.session, operationId,
      intentRevision: node.intentRevision, tracks: { ...off, camera: enabled } });
    await command(node, "CommitOperation", { operationId });
  },
  async startCameraWithdrawal() {
    // Keep trusted indicator teardown from changing the receiver viewport epoch.
    document.querySelector("#controls").style.height = "140px";
    await this.startPair();
    // This independent capture session must end, not the receive-only answerer.
    const camera = createNode(2);
    nodes.push(camera);
    camera.observer = observe(camera);
    camera.backend.attach(camera.runtime);
    await command(camera, "OpenSession", {
      sessionId: camera.session, operationId: id(202), tracks: { ...off, camera: true },
    });
    await command(camera, "CommitOperation", { operationId: id(202) });
    const receiver = nodes[1];
    await command(receiver, "SetSurfaces", remoteCameraSurface(receiver, id(1)));
  },
  decodedFrames() {
    return nodes[1].mount.querySelector("video")?.getVideoPlaybackQuality().totalVideoFrames ?? 0;
  },
  async prepareDelayedAnswererCamera() {
    const node = nodes[1];
    delayNextCapture = true;
    this.pendingOperationId = id(300 + Number(++node.intentRevision));
    this.pending = node.backend.mediaBackendCommand(product, node.runtime, {
      tag: "SetTracks", value: { sessionId: node.session, operationId: this.pendingOperationId,
        intentRevision: node.intentRevision, tracks: { ...off, camera: true } },
    });
  },
  async withdrawCamera() {
    // Exercise the trusted OS-observer entry point, not a simulated OS setting.
    for (const node of nodes.slice(1)) node.backend.revokePermission(node.runtime, "Camera", "OperatingSystem");
    // Cancellation must settle without waiting for foreign capture completion.
    const pending = await this.pending;
    const receiver = nodes[1];
    const commit = await receiver.backend.mediaBackendCommand(product, receiver.runtime, {
      tag: "CommitOperation", value: { operationId: this.pendingOperationId },
    });
    const camera = nodes[2];
    const committed = await camera.backend.mediaBackendCommand(product, camera.runtime, {
      tag: "CommitOperation", value: { operationId: id(202) },
    });
    const session = await camera.backend.mediaBackendCommand(product, camera.runtime, {
      tag: "SetTracks", value: { sessionId: camera.session, operationId: id(500),
        intentRevision: 1n, tracks: off },
    });
    const stoppedCapture = captures[1].getTracks().map((track) => track.readyState);
    releaseCapture();
    releaseCapture = undefined;
    await new Promise((resolve) => setTimeout(resolve, 0));
    return { pending, commit, session, committed: committed.tag, stoppedCapture,
      lateCapture: captures[2].getTracks().map((track) => track.readyState) };
  },
  withdrawalSnapshot() {
    return { failures, captureRequests: captures.length,
      peer: nodes[1].peer, camera: nodes[1].remote?.camera,
      revocations: nodes.slice(1).map((node) => node.revocations) };
  },
  async prepareDelayedCapture() {
    const node = createNode(0);
    nodes.push(node);
    node.observer = observe(node);
    node.backend.attach(node.runtime);
    delayNextCapture = true;
    this.pending = node.backend.mediaBackendCommand(product, node.runtime, {
      tag: "OpenSession", value: { sessionId: node.session, operationId: id(400), tracks: { ...off, camera: true } },
    });
  },
  captureWaiting() { return !!releaseCapture; },
  async cancelDelayedCapture() {
    await command(nodes[0], "CancelOperation", { operationId: id(400) });
    releaseCapture();
    const result = await this.pending;
    // The foreign capture completion runs after cancellation has already won.
    await new Promise((resolve) => setTimeout(resolve, 0));
    return { result: result.tag, tracks: captures.flatMap((stream) => stream.getTracks().map((track) => track.readyState)) };
  },
  // Models an engine that ignores `iceTransportPolicy: "relay"`: the native
  // agent then gathers host candidates, which the backend must never signal.
  ignoreRelayPolicy() {
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(config) {
        super({ ...config, iceTransportPolicy: "all" });
        this.addEventListener("icecandidate", (event) => {
          if (event.candidate?.candidate) gathered.add(candidateType(event.candidate.candidate));
        });
      }
    };
  },
  iceSnapshot() {
    return { gathered: [...gathered].sort(), signaled: [...signaled].sort() };
  },
  snapshot() {
    return { failures, captureRequests: captures.length,
      peers: nodes.map((node) => node.peer), remoteCameras: nodes.map((node) => node.remote?.camera) };
  },
  dispose() { nodes.forEach((node) => node.backend.dispose()); },
};
