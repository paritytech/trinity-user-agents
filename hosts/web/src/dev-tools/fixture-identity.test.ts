import { describe, expect, test } from "bun:test";
import type { Person } from "../invitations/ports.js";
import { cryptoRandom, digest } from "../invitations/sealing.js";
import {
  FixtureIdentityAuth,
  fixtureCredential,
  publicIdentity,
} from "./fixture-identity.js";
import { TEST_USERS, testUserKey } from "./test-users.js";

const credential = (name: string, agreement = name) =>
  fixtureCredential(digest("signing", name), digest("agreement", agreement));
const personOf = (name: string, agreement = name): Person => ({
  id: name,
  name,
  handle: `${name}.example`,
  identity: publicIdentity(credential(name, agreement)),
});
const auth = (name: string, agreement = name) =>
  new FixtureIdentityAuth(credential(name, agreement), cryptoRandom);
const transcript = digest("transcript", "one");

describe("the reference fixture identity profile", () => {
  test("gives v3's evidence fields, and checks only for its peer, kind and transcript", () => {
    const evidence = auth("ada").evidence(
      "Request",
      transcript,
      personOf("lin"),
    );
    expect(evidence).toEqual({
      kind: "Request",
      identityMac: expect.stringMatching(/^0x[0-9a-f]{64}$/),
      messageSignature: expect.stringMatching(/^0x[0-9a-f]{128}$/),
    });
    const lin = auth("lin");
    expect(lin.check("Request", transcript, evidence, personOf("ada"))).toBe(
      true,
    );
    expect(lin.check("Reply", transcript, evidence, personOf("ada"))).toBe(
      false,
    );
    expect(
      lin.check(
        "Request",
        digest("transcript", "two"),
        evidence,
        personOf("ada"),
      ),
    ).toBe(false);
    expect(
      auth("kim").check("Request", transcript, evidence, personOf("ada")),
    ).toBe(false);
  });

  test("the MAC and the signature each matter, and evidence does not check in the opposite direction", () => {
    const lin = auth("lin");
    // Ada's signing key with another agreement key: a good signature, a wrong MAC.
    const wrongMac = auth("ada", "other").evidence(
      "Reply",
      transcript,
      personOf("lin"),
    );
    expect(lin.check("Reply", transcript, wrongMac, personOf("ada"))).toBe(
      false,
    );
    // Kim's signature with Ada's agreement key: a right MAC, a wrong signature.
    const wrongSigner = auth("kim", "ada").evidence(
      "Reply",
      transcript,
      personOf("lin"),
    );
    expect(lin.check("Reply", transcript, wrongSigner, personOf("ada"))).toBe(
      false,
    );
    // The MAC key is shared by both sides, so only the signature tells Lin's own evidence from Ada's.
    const linsOwn = lin.evidence("Reply", transcript, personOf("ada"));
    expect(lin.check("Reply", transcript, linsOwn, personOf("ada"))).toBe(
      false,
    );
    expect(
      auth("ada").check("Reply", transcript, linsOwn, personOf("lin")),
    ).toBe(true);
  });
});

describe("test user lists", () => {
  test("the sent and received lists are v2; earlier v1 lists are not read", () => {
    const [alice] = TEST_USERS;
    expect(testUserKey("paseo", alice!.id, "outbox")).toBe(
      "truapi-web-signing-host:network:paseo:test-user:alice:outbox:v2",
    );
    expect(testUserKey("paseo", alice!.id, "received")).toBe(
      "truapi-web-signing-host:network:paseo:test-user:alice:received:v2",
    );
    expect(testUserKey("paseo", alice!.id, "notifications")).toBe(
      "truapi-web-signing-host:network:paseo:test-user:alice:notifications:v1",
    );
  });
});
