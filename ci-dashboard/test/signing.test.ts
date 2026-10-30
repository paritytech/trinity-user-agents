import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  currentProfiles,
  parseCertificates,
  parseProfile,
  stage,
  summary,
  type SigningItem,
} from "../src/signing";

const now = new Date("2026-10-25T06:00:00Z");
const inDays = (days: number) => new Date(now.getTime() + days * 86_400_000);

/** A .mobileprovision as match installs it: binary CMS around a plist. */
function profile(name: string, uuid: string, created: string, expires: string) {
  const plist = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>AppIDName</key><string>Polkadot</string>
  <key>CreationDate</key><date>${created}</date>
  <key>Entitlements</key><dict><key>application-identifier</key><string>TEAM.io.parity.polkadotapp</string></dict>
  <key>ExpirationDate</key><date>${expires}</date>
  <key>Name</key><string>${name}</string>
  <key>UUID</key><string>${uuid}</string>
</dict></plist>`;
  return new Uint8Array([
    0x30,
    0x80,
    0x06,
    0x09,
    0xff,
    0x00,
    ...new TextEncoder().encode(plist),
    0x00,
    0xa0,
    0x82,
  ]);
}

const item = (id: string, days: number): SigningItem => ({
  id,
  kind: "profile",
  name: `match AppStore ${id}`,
  expires: inDays(days),
});

describe("parseProfile", () => {
  test("reads the plist out of the signed envelope", () => {
    const parsed = parseProfile(
      profile(
        "match AppStore io.parity.polkadotapp",
        "uuid-1",
        "2026-01-01T00:00:00Z",
        "2027-01-01T00:00:00Z",
      ),
    );
    expect(parsed).toEqual({
      id: "uuid-1",
      kind: "profile",
      name: "match AppStore io.parity.polkadotapp",
      expires: new Date("2027-01-01T00:00:00Z"),
      created: new Date("2026-01-01T00:00:00Z"),
      appId: "io.parity.polkadotapp",
    });
  });

  test("returns nothing for a file that is not a profile", () => {
    expect(parseProfile(new TextEncoder().encode("not a profile"))).toBe(
      undefined,
    );
  });
});

describe("currentProfiles", () => {
  test("keeps the newest copy of each match profile, and only those", () => {
    const parse = (name: string, uuid: string, created: string) =>
      parseProfile(profile(name, uuid, created, "2027-01-01T00:00:00Z"))!;
    const kept = currentProfiles([
      parse(
        "match AppStore io.parity.polkadotapp",
        "old",
        "2025-01-01T00:00:00Z",
      ),
      parse(
        "match AppStore io.parity.polkadotapp",
        "new",
        "2026-01-01T00:00:00Z",
      ),
      parse("Someone else's profile", "other", "2026-02-01T00:00:00Z"),
    ]);
    expect(kept.map((p) => p.id)).toEqual(["new"]);
  });

  test("keeps only the apps asked for", () => {
    const own = parseProfile(
      profile(
        "match AppStore io.parity.polkadotapp",
        "own",
        "2026-01-01T00:00:00Z",
        "2027-01-01T00:00:00Z",
      ),
    )!;
    const foreign = {
      ...own,
      id: "foreign",
      name: "match AppStore com.other.app",
      appId: "com.other.app",
    };
    expect(
      currentProfiles([own, foreign], ["io.parity.polkadotapp"]).map(
        (p) => p.id,
      ),
    ).toEqual(["own"]);
    expect(currentProfiles([own, foreign]).map((p) => p.id)).toEqual([
      "own",
      "foreign",
    ]);
  });
});

describe("parseCertificates", () => {
  test("keeps the signing identity and leaves out the CA", () => {
    const pem = readFileSync(
      join(import.meta.dir, "fixtures/certificates.pem"),
      "utf8",
    );
    const certificates = parseCertificates(pem + pem);
    expect(certificates.map((c) => c.name)).toEqual([
      "Apple Distribution: Example (TEAM123456)",
    ]);
    expect(certificates[0].kind).toBe("certificate");
    expect(certificates[0].expires.getUTCFullYear()).toBeGreaterThan(2026);
  });
});

describe("stage", () => {
  test("counts the thresholds passed", () => {
    expect(stage(30)).toBe(0);
    expect(stage(29)).toBe(1);
    expect(stage(14)).toBe(1);
    expect(stage(13)).toBe(2);
    expect(stage(7)).toBe(2);
    expect(stage(6)).toBe(3);
    expect(stage(-1)).toBe(3);
  });
});

describe("summary", () => {
  test("lists soonest first", () => {
    const table = summary([item("late", 200), item("soon", 5)], now);
    expect(table.indexOf("soon")).toBeLessThan(table.indexOf("late"));
  });

  test("marks each item by how close it is", () => {
    const table = summary(
      [item("a", 20), item("b", 10), item("c", 1), item("d", -2)],
      now,
    );
    expect(table).toContain(
      "| ⚠️ | profile | match AppStore a | expires in 20 days (2026-11-14) |",
    );
    expect(table).toContain(
      "| 🚨 | profile | match AppStore b | expires in 10 days (2026-11-04) |",
    );
    expect(table).toContain(
      "| ❌ | profile | match AppStore c | expires in 1 day (2026-10-26) |",
    );
    expect(table).toContain(
      "| ❌ | profile | match AppStore d | expired on 2026-10-23 |",
    );
  });
});

// validate-signing-credentials.yml runs this script on a shared Mac with the
// fetched certificates and profiles, and nothing else exercises it there.
describe("run-signing", () => {
  const script = join(import.meta.dir, "../src/run-signing.ts");
  const pem = join(import.meta.dir, "fixtures/certificates.pem");

  async function run(profiles: [string, Date][]) {
    const dir = await mkdtemp(join(tmpdir(), "ci-dashboard-profiles-"));
    for (const [uuid, expires] of profiles) {
      await writeFile(
        join(dir, `${uuid}.mobileprovision`),
        profile(
          `match AppStore io.parity.polkadotapp ${uuid}`,
          uuid,
          "2026-01-01T00:00:00Z",
          expires.toISOString(),
        ),
      );
    }
    const result = Bun.spawnSync([process.execPath, script, pem, dir], {
      env: {
        PATH: process.env.PATH ?? "",
        SIGNING_APP_IDENTIFIERS: "io.parity.polkadotapp",
      },
    });
    return {
      code: result.exitCode,
      out: result.stdout.toString() + result.stderr.toString(),
    };
  }

  const fromToday = (days: number) => new Date(Date.now() + days * 86_400_000);

  test("passes and lists each item while all have a week or more", async () => {
    const { code, out } = await run([["fine", fromToday(60)]]);
    expect(code).toBe(0);
    expect(out).toContain("Apple Distribution: Example (TEAM123456)");
    expect(out).toContain("match AppStore io.parity.polkadotapp fine");
  });

  test("fails while an item has under a week left", async () => {
    const { code, out } = await run([["soon", fromToday(5)]]);
    expect(code).toBe(1);
    expect(out).toContain("::error::match AppStore io.parity.polkadotapp soon");
  });

  test("fails when it finds no profiles", async () => {
    const { code } = await run([]);
    expect(code).not.toBe(0);
  });
});
