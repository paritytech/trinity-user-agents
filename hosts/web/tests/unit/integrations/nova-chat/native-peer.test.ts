import { describe, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as sr25519 from "@scure/sr25519";
import { hexToBytes, type HexString } from "@parity/truapi/scale";
import { x25519PublicKey } from "../../../../bridges/primitives/crypto.js";
import {
  aeadKey,
  agree,
  seal,
} from "../../../../src/integrations/nova-chat/crypto.js";
import {
  isRefused,
  type NovaBoundary,
} from "../../../../src/integrations/nova-chat/boundary.js";
import {
  protocolDay,
  requestDayTopic,
  requestFullTopic,
  requestSessionTopic,
} from "../../../../src/integrations/nova-chat/topics.js";
import {
  encodeRequestDecrypted,
  encodeRequestEnvelope,
  encodeMessage,
  requestProofPayload,
} from "../../../../bridges/nova-chat/wire.js";
import {
  A_CHAT,
  A_DEVICE,
  A_ID,
  B_ID,
  B_IDENTITY_SECRET,
  B_PEER,
  B_PER_CHAT_SECRET,
  CONVERSATION_SECRET,
  NOTIFICATION_SECRET,
  asPeerJson,
  boundaryA,
  fromPeer,
  hex,
  seed,
  signAs,
} from "./fixtures.js";

/**
 * Nova Chat's host key boundary against `native-peer.py`
 * (`tests/unit/fixtures/nova-chat/`): a second reading of the apps' wire,
 * written earlier and apart from this code, from the iOS and Android
 * sources. It shares that source reading, cannot check sr25519 signatures,
 * and is not an app: passing it shows two readings agree, not that a real
 * build reads us.
 *
 * Every run reads the statements the peer made, pinned in
 * `native-peer.json`. With `NOVA_PEER=1` the peer runs through `uv`, makes
 * them again, and also reads what this code emits; that run also checks the
 * pinned file is still what the peer makes.
 */

const FIXTURES = new URL("../../fixtures/nova-chat/", import.meta.url);
const RUN_PEER = process.env.NOVA_PEER === "1";
const T0 = 1_800_000_300_000;
const params = {
  A_ID: hex(A_ID),
  A_CHAT: hex(A_CHAT),
  A_DEV: hex(A_DEVICE),
  B_ID: hex(B_ID),
};

interface PeerEntry {
  topic: string;
  channel: string;
  data: string;
}
interface Peer {
  acceptance: PeerEntry;
  text: PeerEntry;
  response: PeerEntry;
  unaddressed: PeerEntry;
  request: { message: string; identityProof: string };
  keys: { chat: string; device: string; identity: string };
}

const dir = RUN_PEER ? mkdtempSync(join(tmpdir(), "nova-peer-")) : "";
const runPeer = (): string =>
  execFileSync(
    "uv",
    ["run", "--quiet", new URL("native-peer.py", FIXTURES).pathname],
    {
      env: {
        ...process.env,
        PEER_DIR: dir,
        PEER_PARAMS: join(dir, "params.json"),
      },
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    },
  );

const pinned = JSON.parse(
  readFileSync(new URL("native-peer.json", FIXTURES), "utf8"),
) as Peer;
if (RUN_PEER) {
  writeFileSync(join(dir, "params.json"), JSON.stringify(params));
  runPeer();
}
const peer: Peer = RUN_PEER
  ? (JSON.parse(readFileSync(join(dir, "native-peer.json"), "utf8")) as Peer)
  : pinned;

/** B's V2 request from the peer's message bytes, signed and sealed here, since the peer has no sr25519. */
function peerRequest() {
  const message = hexToBytes(peer.request.message as HexString);
  const signature = sr25519.sign(
    B_IDENTITY_SECRET,
    requestProofPayload(message, A_ID),
  );
  const ephemeral = seed(0x55);
  const shared = agree(ephemeral, A_CHAT);
  return signAs(NOTIFICATION_SECRET, {
    expiry: 1n,
    channel: null,
    topics: [
      requestDayTopic(A_ID, protocolDay(1_800_000_050_000)),
      requestFullTopic(A_ID),
      requestSessionTopic(
        agree(seed(0x21), hexToBytes(peer.keys.chat as HexString)),
        B_ID,
        A_ID,
      ),
    ],
    data: encodeRequestEnvelope(
      x25519PublicKey(ephemeral),
      seal(aeadKey(shared), encodeRequestDecrypted(message, signature, B_ID)),
    ),
  });
}

async function outgoingChat(): Promise<NovaBoundary> {
  const host = boundaryA();
  const sent = await host.request(B_ID, {
    requestId: "a-req-0001",
    nowMs: T0,
    expiry: 1n,
    welcome: "hello native",
  });
  if (isRefused(sent)) throw new Error(sent.refused);
  return host;
}

describe("what the peer sends, read by this code", () => {
  test("pinned and fresh peer statements agree when the peer runs", () => {
    if (RUN_PEER) expect(peer).toEqual(pinned);
  });

  test("its kind 20 establishes our request, and its device-route text opens, signed by a per-chat account", async () => {
    const host = await outgoingChat();
    expect(
      host.openAcceptance(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, fromPeer(peer.acceptance)),
      ),
    ).toEqual({ established: true });
    expect(
      host.openFrame(B_PEER, signAs(B_PER_CHAT_SECRET, fromPeer(peer.text))),
    ).toMatchObject({
      kind: "Request",
      requestId: "b-flush-0001",
      messages: [
        {
          id: "b-msg-0001",
          content: { kind: "Text", text: "hello from native" },
        },
      ],
    });
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, fromPeer(peer.response)),
      ),
    ).toEqual({ kind: "Response", requestId: "a-flush-0003", code: 0 });
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, fromPeer(peer.unaddressed)),
      ),
    ).toEqual({ refused: "no entry for this device" });
  });

  test("its V2 request opens, with its identity proof and welcome", async () => {
    expect(await boundaryA().openRequest(peerRequest())).toMatchObject({
      peer: B_PEER,
      requestId: "b-req-0001",
      welcome: "hi from native",
      duplicate: false,
    });
  });
});

describe.if(RUN_PEER)("what this code sends, read by the peer", () => {
  test("our request, kind 20, text and acks", async () => {
    const emitted: Record<string, Record<string, unknown>> = { t1: {}, t2: {} };
    const conversation = (statement: Parameters<typeof asPeerJson>[0]) =>
      asPeerJson(statement);
    const host = await outgoingChat();
    const sent = await boundaryA().request(B_ID, {
      requestId: "a-req-0001",
      nowMs: T0,
      expiry: 1n,
      welcome: "hello native",
    });
    if (isRefused(sent)) throw new Error(sent.refused);
    emitted.t1!.request = conversation(
      signAs(NOTIFICATION_SECRET, sent.statement),
    );
    host.openAcceptance(
      B_PEER,
      signAs(B_PER_CHAT_SECRET, fromPeer(peer.acceptance)),
    );
    const seal1 = (frame: Parameters<NovaBoundary["sealFrame"]>[1]) => {
      const sealed = host.sealFrame(B_PEER, frame, 1n);
      if (isRefused(sealed)) throw new Error(sealed.refused);
      return conversation(signAs(CONVERSATION_SECRET, sealed));
    };
    emitted.t1!.ack = seal1({
      kind: "Response",
      requestId: "b-flush-0001",
      code: 0,
    });
    emitted.t1!.text = seal1({
      kind: "Request",
      requestId: "a-flush-0003",
      messages: [
        encodeMessage({
          id: "a-msg-0001",
          timestamp: BigInt(T0),
          content: { kind: "Text", text: "hello from web" },
        }),
      ],
    });

    const host2 = boundaryA();
    const incoming = await host2.openRequest(peerRequest());
    if (isRefused(incoming)) throw new Error(incoming.refused);
    const acceptance = host2.accept(B_PEER, {
      messageId: "a-acc-0001",
      nowMs: T0,
      expiry: 1n,
    });
    if (isRefused(acceptance)) throw new Error(acceptance.refused);
    host2.accepted(B_PEER);
    emitted.t2!.acceptance = conversation(
      signAs(NOTIFICATION_SECRET, acceptance),
    );
    const seal2 = (frame: Parameters<NovaBoundary["sealFrame"]>[1]) => {
      const sealed = host2.sealFrame(B_PEER, frame, 1n);
      if (isRefused(sealed)) throw new Error(sealed.refused);
      return conversation(signAs(CONVERSATION_SECRET, sealed));
    };
    emitted.t2!.ack = seal2({
      kind: "Response",
      requestId: "b-flush-0001",
      code: 0,
    });
    emitted.t2!.text = seal2({
      kind: "Request",
      requestId: "a-flush-0004",
      messages: [
        encodeMessage({
          id: "a-msg-0002",
          timestamp: BigInt(T0),
          content: { kind: "Text", text: "reply from web" },
        }),
      ],
    });
    writeFileSync(
      join(dir, "native-peer.emitted.json"),
      JSON.stringify(emitted),
    );
    expect(JSON.parse(runPeer())).toEqual({
      "t1.request": { id: "a-req-0001", welcome: "hello native" },
      "t1.text": {
        requestId: "a-flush-0003",
        messages: [{ id: "a-msg-0001", kind: 0, text: "hello from web" }],
      },
      "t1.ack": { requestId: "b-flush-0001", code: 0 },
      "t2.acceptance": { requestId: "b-req-0001", transportId: "a-acc-0001" },
      "t2.ack": { requestId: "b-flush-0001", code: 0 },
      "t2.text": {
        requestId: "a-flush-0004",
        messages: [{ id: "a-msg-0002", kind: 0, text: "reply from web" }],
      },
    });
  });

  test("the peer refuses a request with its topics out of order", async () => {
    const sent = await boundaryA().request(B_ID, {
      requestId: "a-req-0001",
      nowMs: T0,
      expiry: 1n,
      welcome: "x",
    });
    if (isRefused(sent)) throw new Error(sent.refused);
    const [day, full, pair] = sent.statement.topics;
    const shuffled = signAs(NOTIFICATION_SECRET, {
      ...sent.statement,
      topics: [full!, day!, pair!],
    });
    writeFileSync(
      join(dir, "native-peer.emitted.json"),
      JSON.stringify({ t1: { request: asPeerJson(shuffled) } }),
    );
    expect(() => runPeer()).toThrow();
  });
});
