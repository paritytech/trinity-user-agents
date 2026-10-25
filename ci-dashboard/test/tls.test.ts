import { describe, expect, test } from "bun:test";

import { checkCertificate, httpsHosts } from "../src/tls";

const now = new Date("2026-10-25T00:00:00Z");
const expiresOn = (iso: string) => async () => new Date(iso);

describe("checkCertificate", () => {
  test("passes with two weeks or more left", async () => {
    const result = await checkCertificate(
      "docs.test",
      now,
      expiresOn("2026-12-25T00:00:00Z"),
    );
    expect(result).toMatchObject({
      name: "docs.test certificate",
      url: "tls://docs.test",
      ok: true,
      detail: "expires in 61 days (2026-12-25)",
    });
  });

  test("fails inside the last two weeks", async () => {
    const result = await checkCertificate(
      "docs.test",
      now,
      expiresOn("2026-11-01T00:00:00Z"),
    );
    expect(result).toMatchObject({
      ok: false,
      detail: "expires in 7 days (2026-11-01)",
    });
  });

  test("fails once expired", async () => {
    const result = await checkCertificate(
      "docs.test",
      now,
      expiresOn("2026-10-20T00:00:00Z"),
    );
    expect(result).toMatchObject({
      ok: false,
      detail: "expired on 2026-10-20",
    });
  });

  test("fails when the handshake does", async () => {
    const result = await checkCertificate("docs.test", now, async () => {
      throw new Error("timed out");
    });
    expect(result).toMatchObject({ ok: false, detail: "timed out" });
  });
});

describe("httpsHosts", () => {
  test("keeps each https host once, in order", () => {
    expect(
      httpsHosts([
        "https://a.test/one",
        "https://b.test/",
        "https://a.test/two",
        "http://c.test/",
      ]),
    ).toEqual(["a.test", "b.test"]);
  });
});
