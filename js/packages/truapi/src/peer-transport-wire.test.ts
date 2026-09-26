import { expect, test } from "bun:test";
import * as S from "./scale.js";
import * as T from "./generated/types.js";
import {
  PEER_TRANSPORT_CLOSE,
  PEER_TRANSPORT_DIAL,
  PEER_TRANSPORT_EVENTS,
  PEER_TRANSPORT_OPEN,
  PEER_TRANSPORT_RECV,
  PEER_TRANSPORT_RESET,
  PEER_TRANSPORT_SEND,
} from "./generated/wire-table.js";
import { decodeWireMessage, encodeWireMessage } from "./transport.js";

// The same bytes `rust/crates/truapi-server/tests/peer_transport_grant.rs`
// pins for the Rust SCALE codec: both sides must agree on the frozen V1 layout.
const GENESIS = "0x353963b9cedfe4ea22038081052a5c151b06b55a4a026a97522cd0320cabf49f" as const;
const LOOPBACK_V4_MAPPED = "0x00000000000000000000ffff7f000001" as const;

test("PeerTransport is namespace 21 with methods 0..6 in contract order", () => {
  const ids = [PEER_TRANSPORT_DIAL, PEER_TRANSPORT_OPEN, PEER_TRANSPORT_SEND, PEER_TRANSPORT_RECV,
    PEER_TRANSPORT_RESET, PEER_TRANSPORT_CLOSE, PEER_TRANSPORT_EVENTS];
  ids.forEach((id, method) => {
    expect(id.trait).toBe(21);
    expect(id.method).toBe(method);
    expect(id.kind).toBe("request");
  });
});

test("dial request encodes genesis, v4-mapped ip, port, ed25519 and optional p256 as Rust does", () => {
  const encoded = T.VersionedHostPeerTransportDialRequest.enc({
    tag: "V1",
    value: { genesis: GENESIS, ip: LOOPBACK_V4_MAPPED, port: 43000, ed25519: `0x${"11".repeat(32)}`, p256: `0x${"02".repeat(33)}` },
  });
  expect([...encoded]).toEqual([
    0,
    ...S.hexToBytes(GENESIS),
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 127, 0, 0, 1,
    0xf8, 0xa7,
    ...new Array<number>(32).fill(0x11),
    1,
    ...new Array<number>(33).fill(0x02),
  ]);
  const withoutP256 = T.VersionedHostPeerTransportDialRequest.enc({
    tag: "V1",
    value: { genesis: GENESIS, ip: LOOPBACK_V4_MAPPED, port: 43000, ed25519: `0x${"11".repeat(32)}`, p256: undefined },
  });
  expect(withoutP256.length).toBe(1 + 32 + 16 + 2 + 32 + 1);
  expect(withoutP256[withoutP256.length - 1]).toBe(0);
  expect(T.VersionedHostPeerTransportDialRequest.dec(withoutP256)).toEqual({
    tag: "V1",
    value: { genesis: GENESIS, ip: LOOPBACK_V4_MAPPED, port: 43000, ed25519: `0x${"11".repeat(32)}`, p256: undefined },
  });
});

test("send, recv and events payloads match the Rust SCALE bytes", () => {
  expect([...T.VersionedHostPeerTransportSendRequest.enc({ tag: "V1", value: { stream: 7, message: "0xaabb", fin: true } })])
    .toEqual([0, 7, 0, 0, 0, 8, 0xaa, 0xbb, 1]);
  expect([...T.VersionedHostPeerTransportRecvResponse.enc({ tag: "V1", value: { message: undefined, fin: false, reset: true } })])
    .toEqual([0, 0, 0, 1]);
  expect([...T.VersionedHostPeerTransportRecvRequest.enc({ tag: "V1", value: { stream: 3, max: 1 << 20 } })])
    .toEqual([0, 3, 0, 0, 0, 0, 0, 0x10, 0]);
  const events = T.VersionedHostPeerTransportEventsResponse.enc({
    tag: "V1",
    value: {
      events: [
        { tag: "ConnClosed", value: { conn: 1 } },
        { tag: "StreamFin", value: { stream: 2 } },
        { tag: "Accepted", value: { conn: 1, stream: 3, kind: 0 } },
      ],
    },
  });
  expect([...events]).toEqual([0, 12, 0, 1, 0, 0, 0, 1, 2, 0, 0, 0, 2, 1, 0, 0, 0, 3, 0, 0, 0, 0]);
  expect([...T.VersionedHostPeerTransportEventsRequest.enc({ tag: "V1", value: undefined })]).toEqual([0]);
  expect([...T.VersionedHostPeerTransportDialError.enc({ tag: "V1", value: "Unreachable" })]).toEqual([0, 3]);
});

test("a NotGranted dial response decodes from a host frame", () => {
  const resultCodec = S.Result(T.VersionedHostPeerTransportDialResponse, S.CallError(T.VersionedHostPeerTransportDialError));
  const value = resultCodec.enc({ success: false, value: { tag: "Domain", value: { tag: "V1", value: "NotGranted" } } });
  const frame = encodeWireMessage({ requestId: "p", payload: { traitId: 21, methodId: 0, messageType: 1, value } });
  if (frame.isErr()) throw frame.error;
  expect([...frame.value]).toEqual([4, 112, 21, 0, 1, 1, 0, 0, 0]);
  const decoded = decodeWireMessage(frame.value);
  if (decoded.isErr()) throw decoded.error;
  expect(decoded.value.requestId).toBe("p");
  expect(resultCodec.dec(decoded.value.payload.value)).toEqual({
    success: false,
    value: { tag: "Domain", value: { tag: "V1", value: "NotGranted" } },
  });
});
