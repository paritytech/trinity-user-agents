import { describe, expect, test } from "bun:test";
import { sha256 } from "@noble/hashes/sha2.js";
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
  webTransportSerial,
} from "./jam-peer-transport-cert.js";

// Two validators of a local network running PolkaJAM dd9af78 with
// jam-explore's `polkajam-webtransport-serial.patch`. `der2072` is the
// certificate each node served during period 2072, dumped with
// `openssl s_client -quic -alpn h3 -showcerts`. The other `distinct` hashes
// come from the patched node's own `net/cert.rs` for those periods; the
// `legacy` (serial 0) hashes from jam-explore `crates/jam-webtransport-cert`,
// whose legacy output equals the stock certificate in `STOCK`.
const PATCHED = [
  {
    id: "v3bl2cgtywlclprhhuc5tumdn4ulhwir2mahkcqm6tkdbyo6v2hba",
    compressed: "023b2c2dccc47689f5e23954f449d9689cae6351d40c1c2520f3538d809daffa04",
    serial2072: 0x20dbcba0be3fb21cn,
    distinct: {
      2071: "45b4746857e99aef73adc8d14599ad772ea2e209fb2b78b24956e3c7d43090f8",
      2072: "31f457efb35cc02a9db8c4b725a20626828dce21648d7b498663cff82131ea49",
      2073: "9e8398400a3c76597747a04b86315f8c5272742065c70d490412e80bd7538417",
    },
    legacy: {
      2071: "8dec8b1989d2d284b2a6ab179ee6c78805907d6b1b97f338fa7a6cf3377b9549",
      2072: "6e7e01e8e40edfeba2a56b4555b5e6dba01cdd4e8a2650bd2890d5ab559a2f97",
      2073: "396d273a20dcf3cf067fbd2ce00482bb86a7c0a33ef7bc278c1fbe930be53401",
    },
    der2072:
      "308201443081f7a003020102020820dbcba0be3fb21c300506032b6570300e310c300a06035504030c036a616d301e170d3236303932333030303030305a170d3236313030353030303030305a300e310c300a06035504030c036a616d3059301306072a8648ce3d020106082a8648ce3d030107034200043b2c2dccc47689f5e23954f449d9689cae6351d40c1c2520f3538d809daffa0498fb2c7ecfde6e286fd1c9203c1c8e9d50f37bce6b571b60d3978617424cfd48a344304230400603551d110439303782357633626c3263677479776c636c7072686875633574756d646e34756c68776972326d61686b63716d36746b6462796f367632686261300506032b657003410000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  },
  {
    id: "oyhjn3rnl7vx235ayvn2f4uhsyorfflk5tnrn5s3d6erxacv3vjla",
    compressed: "03f8a4b6635bbf5ebd3bc0b5e9c2e991d8c55296eab3c5d6e51e9ec40b44dd352d",
    serial2072: 0x4d2e1400cc6928d7n,
    distinct: {
      2071: "6322b9d00cf2522232687f7bd0bf78a5127e96dd74d1b162870cb7cf5529b7b6",
      2072: "7fa6a214c42db09966ee8b673ba989a0b8235963972bbf635cef03be2f60c1d1",
      2073: "1f21b2db288a5e7e5522f947b2224fa87cdcb28acf54fa64f8699ce9145514ce",
    },
    legacy: {
      2071: "f82ad0ada188038205f02ec01311ac0570ec5a7e42a3eff9eebfd047d75557f4",
      2072: "5cc86b9b585a3d91f049084caadc9ab66e2d3d75bd4551e4a2dc9d418675b423",
      2073: "08cab2920d2fc1914fcdd0e0ba3399b42a7a4199a2c3ec19048bfd4f735f6cfe",
    },
    der2072:
      "308201443081f7a00302010202084d2e1400cc6928d7300506032b6570300e310c300a06035504030c036a616d301e170d3236303932333030303030305a170d3236313030353030303030305a300e310c300a06035504030c036a616d3059301306072a8648ce3d020106082a8648ce3d03010703420004f8a4b6635bbf5ebd3bc0b5e9c2e991d8c55296eab3c5d6e51e9ec40b44dd352df4537bba2c6db972303b6c2ba95d5c9ae402f556032e2f058e1a569ffabd457fa344304230400603551d110439303782356f79686a6e33726e6c3776783233356179766e326634756873796f7266666c6b35746e726e357333643665727861637633766a6c61300506032b657003410000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
  },
] as const;

// Certificate a stock JAM-TEST-INSTANCE validator served during period 2072.
const STOCK = {
  compressed: "03aac17e3833a6679e7064934a6f2a2bc9450d3b2b779a9930102ae16a4f16062e",
  der2072:
    "3082013d3081f0a003020102020100300506032b6570300e310c300a06035504030c036a616d301e170d3236303932333030303030305a170d3236313030353030303030305a300e310c300a06035504030c036a616d3059301306072a8648ce3d020106082a8648ce3d03010703420004aac17e3833a6679e7064934a6f2a2bc9450d3b2b779a9930102ae16a4f16062ef93e4094a88912344d64606e51e6ec210633140ad0d6d3c3d292690171463813a344304230400603551d110439303782356f6b6e713568346d6767357a346a79726d7475733667766d666a6f723271356d66787467746a7961636b6a797677687a6367716c61300506032b657003410000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
} as const;

describe("PolkaJAM peer id text", () => {
  test("decodes P-256 ids to the compressed point and back", () => {
    for (const vector of PATCHED) {
      const compressed = p256IdToCompressed(vector.id);
      expect(bytesToHex(compressed)).toBe(vector.compressed);
      expect(peerIdText(vector.id[0]!, compressed.subarray(1))).toBe(vector.id);
    }
  });

  test("decodes Ed25519 ids and rejects the wrong prefix", () => {
    const key = ed25519IdToKey("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra");
    expect(peerIdText("e", key)).toBe("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra");
    expect(() => ed25519IdToKey(PATCHED[0].id)).toThrow("begin with 'e'");
    expect(() => p256IdToCompressed("e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra")).toThrow(
      "'o' or 'v'",
    );
  });
});

describe("P-256 decompression", () => {
  test("matches the uncompressed point the node embedded", () => {
    for (const vector of PATCHED) {
      const point = decompressP256(hexToBytes(vector.compressed));
      // The SPKI BIT STRING carries 0x04 ‖ x ‖ y.
      const index = vector.der2072.indexOf("03420004") + 6;
      expect(bytesToHex(point)).toBe(vector.der2072.slice(index, index + 130));
    }
  });

  test("rejects off-curve x", () => {
    const bad = hexToBytes(PATCHED[0].compressed);
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
  test("reproduces the certificates real nodes served byte for byte", () => {
    for (const vector of PATCHED) {
      const compressed = hexToBytes(vector.compressed);
      expect(webTransportSerial(compressed, 2072)).toBe(vector.serial2072);
      expect(bytesToHex(webTransportCertificateDer(compressed, 2072, "distinct"))).toBe(vector.der2072);
    }
    expect(bytesToHex(webTransportCertificateDer(hexToBytes(STOCK.compressed), 2072, "legacy"))).toBe(
      STOCK.der2072,
    );
  });

  test("encodes a serial with a zero top byte as a minimal positive INTEGER", () => {
    // Serial 0x00ada8453f66c43e: drop the zero byte, then pad because 0xad has the sign bit set.
    const compressed = hexToBytes(PATCHED[0].compressed);
    expect(webTransportSerial(compressed, 2099)).toBe(0x00ada8453f66c43en);
    const der = webTransportCertificateDer(compressed, 2099, "distinct");
    expect(bytesToHex(der.subarray(12, 22))).toBe("020800ada8453f66c43e");
    // Hash of the patched node's own certificate for this period.
    expect(bytesToHex(sha256(der))).toBe("2d1583ea892ae8c792fc499d2091f91d9ef9c4973ff3002eea577e639bee8800");
  });

  test("pins both serial variants for the current period and both neighbours", () => {
    for (const vector of PATCHED) {
      const compressed = hexToBytes(vector.compressed);
      for (const period of [2071, 2072, 2073] as const) {
        expect(bytesToHex(webTransportCertificateHash(compressed, period, "distinct"))).toBe(vector.distinct[period]);
        expect(bytesToHex(webTransportCertificateHash(compressed, period, "legacy"))).toBe(vector.legacy[period]);
      }
      expect(webTransportCertificateHashes(compressed, 1_790_380_800).map(bytesToHex)).toEqual([
        vector.distinct[2071],
        vector.legacy[2071],
        vector.distinct[2072],
        vector.legacy[2072],
        vector.distinct[2073],
        vector.legacy[2073],
      ]);
    }
  });
});
