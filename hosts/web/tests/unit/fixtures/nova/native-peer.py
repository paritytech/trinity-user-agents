# /// script
# dependencies = ["cryptography"]
# ///
"""A stand-in for a native Polkadot app peer ("B"), written independently of the
TypeScript Nova engine under test. It follows the apps' wire as read from the
public iOS and Android sources (chat-spec mds.md and RFC-0005; iOS
`MultiDeviceStatementDataCoder`, `PeerSessionFactory`, `IdentityProofFactory`;
Android `MultiDeviceEnvelopeEncryption`, `IdentityProofCodec`). It is not a
native binary: it shows that the engine's bytes match a second, separately
written reading of the same wire.

    uv run tests/unit/fixtures/nova/native-peer.py

reads `native-peer.emitted.json` (the statements the engine emitted, written by
`NOVA_VECTORS_WRITE=1 bun test tests/unit/chat/nova/native-interop.test.ts`),
decrypts and checks each one as B would, and writes `native-peer.json`: the
statements B sends to the engine. Every key here is synthetic.
"""
import hashlib, json, os, struct, sys
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

HERE = os.path.dirname(os.path.abspath(__file__))

def h(s): return bytes.fromhex(s[2:])
def x(b): return "0x" + b.hex()

# The engine's side, as the test derives it from a synthetic phrase.
A_ID = h("0x3ac1b80910fa1a633577db96e5c77fca970a8e9b3e09b5db005e4563268a3336")
A_CHAT = h("0x7e65a3cffd41e39eaab65201ea848d3c50a6e7c907382a31f41ea535279d675b")
A_DEV = h("0xe7cc20ade0585e02c18da1030fe4c254f65d333b75ea59da05f995d27cfaa137")
# B's identity account is a real sr25519 key in the test (it signs B's request);
# its chat and device keys are the synthetic X25519 keys below.
B_ID = h("0x2669721a953aaa9994052d4e0652e18c694bd6fad91ec32989b47554b3bd6f2d")
B_CHAT = X25519PrivateKey.from_private_bytes(bytes([3]) * 32)
B_DEV = X25519PrivateKey.from_private_bytes(bytes([4]) * 32)

def raw_pub(k): return k.public_key().public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
def ecdh(k, peer): return k.exchange(X25519PublicKey.from_public_bytes(peer))
def b2b(msg, key=b""): return hashlib.blake2b(msg, digest_size=32, key=key).digest()
def hkdf(secret): return HKDF(algorithm=hashes.SHA256(), length=32, salt=None, info=b"").derive(secret)
def seal(key, nonce, plain): return nonce + ChaCha20Poly1305(key).encrypt(nonce, plain, None)
def unseal(key, sealed): return ChaCha20Poly1305(key).decrypt(sealed[:12], sealed[12:], None)

def compact(n):
    if n < 1 << 6: return bytes([n << 2])
    if n < 1 << 14: return bytes([((n << 2) | 1) & 0xFF, n >> 6])
    return struct.pack("<I", (n << 2) | 2)
def vec(b): return compact(len(b)) + b
def string(s): return vec(s.encode())
def u64(n): return struct.pack("<Q", n)

class Reader:
    def __init__(self, b): self.b, self.i = b, 0
    def take(self, n):
        out = self.b[self.i:self.i + n]; assert len(out) == n, "short"; self.i += n; return out
    def u8(self): return self.take(1)[0]
    def u64(self): return struct.unpack("<Q", self.take(8))[0]
    def compact(self):
        m = self.u8(); k = m & 3
        if k == 0: return m >> 2
        if k == 1: return (m >> 2) | (self.u8() << 6)
        assert k == 2; return (m | (self.u8() << 8) | (self.u8() << 16) | (self.u8() << 24)) >> 2
    def vec(self): return self.take(self.compact())
    def string(self): return self.vec().decode()
    def done(self): assert self.i == len(self.b), "bytes left"

def session_topic(secret, first, second): return b2b(b"session" + first + second + b"/" + b"/", secret)
def channel(topic, kind): return b2b(kind, topic)

# ---- What B sends to the engine --------------------------------------------

B_CHAT_PUB, B_DEV_PUB = raw_pub(B_CHAT), raw_pub(B_DEV)

def message(mid, ts, tag, body): return string(mid) + u64(ts) + bytes([0, tag]) + body
def text_message(mid, ts, text): return message(mid, ts, 0, string(text))

def identity_statement(inner_messages, request_id):
    """B -> A on the identity route (a plain Request under the identity secret)."""
    secret = ecdh(B_CHAT, A_CHAT)
    topic = session_topic(secret, B_ID, A_ID)
    data = bytes([0]) + string(request_id) + compact(len(inner_messages)) + b"".join(vec(m) for m in inner_messages)
    return {"topic": x(topic), "channel": x(channel(topic, b"request")),
            "data": x(seal(hkdf(secret), bytes(range(41, 53)), data))}

def device_envelope(tag, inner, recipient_account, nonce_base=1):
    one_time = bytes(range(100 + tag, 132 + tag))
    wrapped = seal(hkdf(ecdh(B_DEV, A_DEV)), bytes(range(21, 33)), one_time)
    return (bytes([tag + 2]) + vec(seal(one_time, bytes(range(nonce_base, nonce_base + 12)), inner))
            + compact(1) + recipient_account + vec(wrapped))

def device_statement(kind, inner, recipient_account=A_ID):
    """B's device -> A: outer key ECDH(B device, A chat), topic session(B device account, A)."""
    secret = ecdh(B_DEV, A_CHAT)
    topic = session_topic(secret, B_ID, A_ID)
    tag = 0 if kind == "request" else 1
    env = device_envelope(tag, inner, recipient_account)
    return {"topic": x(topic), "channel": x(channel(topic, kind.encode())),
            "data": x(seal(hkdf(secret), bytes(range(41, 53)), env))}

def peer_statements():
    out = {}
    # B accepts A's request "a-req-0001" (kind 20), alone on the identity route.
    accepted = message("b-acc-0001", 1_800_000_100_000, 20, string("a-req-0001") + B_ID + B_DEV_PUB)
    out["acceptance"] = identity_statement([accepted], "b-acc-0001")
    # B's text, as a device-route request.
    inner = string("b-flush-0001") + compact(1) + vec(text_message("b-msg-0001", 1_800_000_200_000, "hello from native"))
    out["text"] = device_statement("request", inner)
    # B's response acknowledging the engine's request "a-flush-0003".
    out["response"] = device_statement("response", string("a-flush-0003") + bytes([0]))
    # An envelope with no entry for the engine's device.
    out["unaddressed"] = device_statement("request", inner, recipient_account=bytes([0xEE]) * 32)
    # B's V2 request to A, as message bytes and identity proof (the test signs and seals them).
    proof = b2b(B_ID + B_ID + string("mds-chat-request"), ecdh(B_CHAT, A_CHAT))
    welcome = bytes([1, 1]) + string("hi from native") + bytes([0])
    req = (string("b-req-0001") + u64(1_800_000_050_000) + bytes([1]) + B_ID + proof + B_DEV_PUB
           + bytes([0]) + welcome)
    out["request"] = {"message": x(req), "identityProof": x(proof)}
    out["keys"] = {"chat": x(B_CHAT_PUB), "device": x(B_DEV_PUB), "identity": x(B_ID)}
    return out

# ---- What B checks in the engine's statements ----------------------------------

def check_request(st):
    eph_and_sealed = Reader(h(st["data"]))
    ephemeral, sealed = eph_and_sealed.vec(), eph_and_sealed.vec(); eph_and_sealed.done()
    plain = unseal(hkdf(ecdh(B_CHAT, ephemeral)), sealed)
    r = Reader(plain)
    mid, ts, tag = r.string(), r.u64(), r.u8()
    assert tag == 1, "a version 2 request"
    identity, proof, device = r.take(32), r.take(32), r.take(32)
    assert identity == A_ID and device == A_DEV
    assert proof == b2b(A_ID + A_ID + string("mds-chat-request"), ecdh(B_CHAT, A_CHAT)), "identity proof"
    assert r.u8() == 0, "no push token"
    assert r.u8() == 1 and r.u8() == 1, "welcome text"
    welcome = r.string(); assert r.u8() == 0
    assert r.u8() == 0, "sr25519 proof"; signature, signer = r.take(64), r.take(32); r.done()
    assert signer == A_ID
    topics = [h(t) for t in st["topics"]]
    day = (ts // 1000 - 1_763_164_800) // 86_400
    assert topics[0] == b2b(string("chat-request") + vec(B_ID) + u64(day)), "day topic"
    assert topics[1] == b2b(string("chat-request") + vec(B_ID)), "request topic"
    assert topics[2] == b2b(b"chat-request" + A_ID + B_ID + b"//", ecdh(B_CHAT, A_CHAT)), "request session topic"
    return {"id": mid, "welcome": welcome}

def open_device(st, expect_kind):
    """A -> B on the device route: outer key ECDH(B chat, A device), envelope entry for B's account."""
    secret = ecdh(B_CHAT, A_DEV)
    topics = [h(t) for t in st["topics"]]
    assert topics == [session_topic(secret, A_ID, B_ID)], "device-route topic"
    assert h(st["channel"]) == channel(topics[0], expect_kind.encode())
    env = Reader(unseal(hkdf(secret), h(st["data"])))
    tag = env.u8(); assert tag == (2 if expect_kind == "request" else 3)
    encrypted = env.vec(); entries = [(env.take(32), env.vec()) for _ in range(env.compact())]; env.done()
    assert len(entries) == 1 and entries[0][0] == B_ID, "one entry, for B's device account"
    one_time = unseal(hkdf(ecdh(B_DEV, A_DEV)), entries[0][1]); assert len(one_time) == 32
    return Reader(unseal(one_time, encrypted))

def check_device_request(st):
    r = open_device(st, "request")
    rid = r.string(); msgs = [Reader(r.vec()) for _ in range(r.compact())]; r.done()
    out = []
    for m in msgs:
        mid, ts, ver, kind = m.string(), m.u64(), m.u8(), m.u8(); assert ver == 0
        out.append({"id": mid, "kind": kind, "text": m.string() if kind == 0 else None})
    return {"requestId": rid, "messages": out}

def check_device_response(st):
    r = open_device(st, "response"); rid, code = r.string(), r.u8(); r.done()
    return {"requestId": rid, "code": code}

def check_acceptance(st):
    secret = ecdh(B_CHAT, A_CHAT)
    topic = session_topic(secret, A_ID, B_ID)
    assert [h(t) for t in st["topics"]] == [topic], "identity-route topic"
    assert h(st["channel"]) == channel(topic, b"request")
    r = Reader(unseal(hkdf(secret), h(st["data"])))
    assert r.u8() == 0; rid = r.string(); messages = [Reader(r.vec()) for _ in range(r.compact())]; r.done()
    assert len(messages) == 1, "exactly one message"
    m = messages[0]; mid, ts, ver, kind = m.string(), m.u64(), m.u8(), m.u8()
    assert ver == 0 and kind == 20 and mid == rid, "DeviceChatAccepted, transport id is the message id"
    accepted = m.string(); account, key = m.take(32), m.take(32); m.done()
    assert account == A_ID and key == A_DEV
    return {"requestId": accepted, "transportId": rid}

if __name__ == "__main__":
    report = {}
    path = os.path.join(HERE, "native-peer.emitted.json")
    if os.path.exists(path):
        e = json.load(open(path))
        report["t1.request"] = check_request(e["t1"]["request"])
        report["t1.text"] = check_device_request(e["t1"]["text"])
        report["t1.ack"] = check_device_response(e["t1"]["ack"])
        report["t2.acceptance"] = check_acceptance(e["t2"]["acceptance"])
        report["t2.ack"] = check_device_response(e["t2"]["ack"])
        report["t2.text"] = check_device_request(e["t2"]["text"])
        print(json.dumps(report, indent=1))
    json.dump(peer_statements(), open(os.path.join(HERE, "native-peer.json"), "w"), indent=1)
    print("native-peer.json written", file=sys.stderr)
