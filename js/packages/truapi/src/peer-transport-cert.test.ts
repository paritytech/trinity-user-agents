import { describe, expect, test } from "bun:test";
import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import {
  decompressP256,
  ed25519IdToKey,
  p256IdToCompressed,
  peerIdText,
  validityBounds,
  validityPeriodAt,
  webTransportCertificateDer,
  webTransportCertificateHash,
  webTransportCertificateHashes,
} from "./peer-transport-cert.js";

// Vectors produced by rcgen 0.14.8 / p256 0.13.2 (the PolkaJAM dd9af78
// lockfile versions) following PolkaJAM's `net/cert.rs`, at unix time
// 1790380800.
const VECTORS = [
  {
    id: "vie5obg5rgcfrtqgw2vm37t7l4mssjdncce33wdf5tfndfjqsg6ba",
    compressed: "028874174c8f469438a1b1bab2fde75f9c4999461382ec6d47e9b3b4511294c607",
    hashes: {
      2071: "ccf30196b29007b42fca6f406363ce17781bab0f011bac47dcbe307e0e6a316d",
      2072: "eb09b6b027f5953cb8ca2e8f296e21052c3423370876e67f1634180ddf99f1ec",
      2073: "8bdfa3a2b7822822f5da33fadfa118d39d05b0a6b1086fc82a62a2209f6fae27",
    },
    der2072:
      "3082013d3081f0a003020102020100300506032b6570300e310c300a06035504030c036a616d301e170d3236303932333030303030305a170d3236313030353030303030305a300e310c300a06035504030c036a616d3059301306072a8648ce3d020106082a8648ce3d030107034200048874174c8f469438a1b1bab2fde75f9c4999461382ec6d47e9b3b4511294c6073947ad8ea69d98ff2844bb02f2231ceb65fadc22aa4f529b602182f6ab5ec924a344304230400603551d11043930378235766965356f62673572676366727471677732766d333774376c346d73736a646e63636533337764663574666e64666a717367366261300506032b657003410000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  },
  {
    id: "o5edmn6gwjzmsyahsffsiu4kp3ao6ufexbp2tpzj2jhuplxbb3dxb",
    compressed: "039d0cd6bcb1293389c191a54844b97a1b384f0bb9e1e9f972d2e9d0b76e087bdc",
    hashes: {
      2071: "13589df87a7a37dd3c800d2a724568d0e5d049e27655cb1d8b6a492268be7bb4",
      2072: "2d25e5bf1695ee00c9a197cef8dfa3daa5ee9d6ddf905eebe3bdf58f464e19cd",
      2073: "1798ee46ac715588b3df8561f85f9717497755eac87077439bc0c84a37443e26",
    },
  },
] as const;

describe("PolkaJAM peer id text", () => {
  test("decodes P-256 ids to the compressed point and back", () => {
    for (const vector of VECTORS) {
      const compressed = p256IdToCompressed(vector.id);
      expect(bytesToHex(compressed)).toBe(vector.compressed);
      expect(peerIdText(vector.id[0]!, compressed.subarray(1))).toBe(vector.id);
    }
  });

  test("decodes Ed25519 ids and rejects the wrong prefix", () => {
    const key = ed25519IdToKey("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra");
    expect(peerIdText("e", key)).toBe("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra");
    expect(() => ed25519IdToKey(VECTORS[0].id)).toThrow("begin with 'e'");
    expect(() => p256IdToCompressed("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra")).toThrow(
      "'o' or 'v'",
    );
  });
});

describe("P-256 decompression", () => {
  test("matches the uncompressed point rcgen embedded", () => {
    const point = decompressP256(hexToBytes(VECTORS[0].compressed));
    // The SPKI BIT STRING in der2072 carries 0x04 ‖ x ‖ y.
    const index = VECTORS[0].der2072.indexOf("03420004") + 6;
    expect(bytesToHex(point)).toBe(VECTORS[0].der2072.slice(index, index + 130));
  });

  test("rejects off-curve x", () => {
    const bad = hexToBytes(VECTORS[0].compressed);
    bad[32] ^= 1;
    expect(() => decompressP256(bad)).toThrow("not on the curve");
  });
});

describe("validity periods", () => {
  test("splits time into padded 10-day windows", () => {
    expect(validityPeriodAt(1_790_380_800)).toBe(2072);
    expect(validityBounds(2072)).toEqual([2072 * 864_000 - 86_400, 2073 * 864_000 + 86_400]);
  });
});

describe("certificate derivation", () => {
  test("reproduces the rcgen DER byte for byte", () => {
    const der = webTransportCertificateDer(hexToBytes(VECTORS[0].compressed), 2072);
    expect(bytesToHex(der)).toBe(VECTORS[0].der2072);
  });

  test("hashes match the Rust cross-check for both y parities", () => {
    for (const vector of VECTORS) {
      const compressed = hexToBytes(vector.compressed);
      for (const [period, hash] of Object.entries(vector.hashes)) {
        expect(bytesToHex(webTransportCertificateHash(compressed, Number(period)))).toBe(hash);
      }
      expect(webTransportCertificateHashes(compressed, 1_790_380_800).map(bytesToHex)).toEqual([
        vector.hashes[2071],
        vector.hashes[2072],
        vector.hashes[2073],
      ]);
    }
  });
});
