import { describe, expect, test } from "bun:test";
import { getPublicKey, secretFromSeed, sign, verify } from "@scure/sr25519";
import { getPolkadotSigner } from "polkadot-api/signer";
import { AccountId } from "polkadot-api";
import { FAUCET_URL } from "../shared/faucet";
import { fromHex } from "../shared/hex";
import {
  FaucetRefused,
  dripMessage,
  parseDripResponse,
  requestDrip,
} from "../worker/faucet";

const NOW = 1_800_000_000_000;
const secret = secretFromSeed(new Uint8Array(32).fill(7));
const publicKey = getPublicKey(secret);
const address = AccountId(0).dec(publicKey);
// The session account's signer, built as getcash builds it.
const signer = getPolkadotSigner(publicKey, "Sr25519", (data) =>
  sign(secret, data),
);
const recipient = { address, signBytes: signer.signBytes };

describe("requestDrip", () => {
  // The faucet takes the signed form without a captcha only when the message
  // names the drip address and a fresh timestamp, and the signature verifies
  // against that address over the message or its <Bytes> wrapping
  // (polkadot-testnet-faucet src/dripper/signatureVerify.ts).
  test("posts a body the faucet's signature check accepts", async () => {
    let posted: { url: string; body: string } | undefined;
    const hash = await requestDrip(
      recipient,
      1500,
      async (url, body) => {
        posted = { url, body };
        return '{"status":"broadcasting"}\n{"hash":"0xab"}\n';
      },
      () => NOW,
    );
    expect(hash).toBe("0xab");
    expect(posted?.url).toBe(FAUCET_URL);
    const { signature, ...body } = JSON.parse(posted!.body) as Record<
      string,
      string
    >;
    expect(body).toEqual({
      address,
      parachain_id: "1500",
      message: `faucet:${address}:${NOW}`,
    });
    const wrapped = new TextEncoder().encode(`<Bytes>${body.message}</Bytes>`);
    expect(verify(wrapped, fromHex(signature!), publicKey)).toBe(true);
  });
});

describe("parseDripResponse", () => {
  test("takes the hash from the stream's last line", () => {
    const stream = [
      '{"status":"broadcasting"}',
      '{"status":"included","hash":"0x15aa","blockHash":"0x9772"}',
      '{"hash":"0x15aa"}',
      "",
    ].join("\n");
    expect(parseDripResponse(stream)).toBe("0x15aa");
  });

  // A quota refusal is told apart: it means this account was dripped already,
  // so the session waits for that drip instead of failing.
  test("marks a quota refusal, and only that one", () => {
    const refusal = (line: string) => {
      try {
        parseDripResponse(line);
      } catch (error) {
        return error;
      }
      return undefined;
    };
    const quota = refusal(
      '{"error":"Requester has reached their daily quota. Only request once per day."}',
    );
    expect(quota).toBeInstanceOf(FaucetRefused);
    expect((quota as FaucetRefused).quota).toBe(true);
    const other = refusal('{"error":"Signature verification failed"}');
    expect(other).toBeInstanceOf(FaucetRefused);
    expect((other as FaucetRefused).quota).toBe(false);
    expect(refusal("")).toBeInstanceOf(FaucetRefused);
    expect(refusal('{"status":"broadcasting"}')).toBeInstanceOf(FaucetRefused);
  });

  test("the message names the address and the time", () => {
    expect(dripMessage("1abc", NOW)).toBe(`faucet:1abc:${NOW}`);
  });
});
