import { describe, expect, it } from "bun:test";
import { localizeTimestamps } from "./locale.js";

const clock = (time: string) => time.match(/\d+/g)?.map(Number);

async function localized(
  instants: string[],
  timeZone = "America/New_York",
  languageTag = "en-GB",
) {
  return (
    await localizeTimestamps({
      languageTag,
      timestampsMs: instants.map((iso) => BigInt(Date.parse(iso))),
      timeZone,
    })
  ).timestamps;
}

describe("timestamp localization", () => {
  it("uses the offset at each instant across spring and autumn DST transitions", async () => {
    const [before, after, firstHour, repeatedHour] = await localized([
      "2024-03-10T06:59:00Z",
      "2024-03-10T07:00:00Z",
      "2024-11-03T05:30:00Z",
      "2024-11-03T06:30:00Z",
    ]);
    expect(clock(before!.time)).toEqual([1, 59]);
    expect(clock(after!.time)).toEqual([3, 0]);
    expect(before!.localDate).toBe("2024-03-10");
    expect(after!.localDate).toBe(before!.localDate);
    expect(clock(firstHour!.time)).toEqual([1, 30]);
    expect(repeatedHour!.time).toBe(firstHour!.time);
    expect(repeatedHour!.dateTime).not.toBe(firstHour!.dateTime);
  });

  it("groups by local midnight, not UTC midnight, independently of locale calendars", async () => {
    const instants = ["2024-01-01T14:59:00Z", "2024-01-01T15:00:00Z"];
    const tokyo = await localized(instants, "Asia/Tokyo", "ar-SA-u-ca-islamic-nu-arab");
    const newYork = await localized(instants);
    expect(tokyo.map((value) => value.localDate)).toEqual(["2024-01-01", "2024-01-02"]);
    expect(newYork.map((value) => value.localDate)).toEqual(["2024-01-01", "2024-01-01"]);
    expect(tokyo[0]!.time).toMatch(/[\u0660-\u0669]/);
    expect(tokyo[0]!.date).toMatch(/[\u0660-\u0669]/);
  });

  it("uses the requested language and zone afresh for the same timestamp", async () => {
    const instants = ["2024-07-01T02:30:00Z"];
    const english = await localized(instants);
    const german = await localized(instants, "America/New_York", "de-DE");
    const berlin = await localized(instants, "Europe/Berlin", "de-DE");
    expect(english[0]!.date).not.toBe(german[0]!.date);
    expect(english[0]!.localDate).toBe(german[0]!.localDate);
    expect(german[0]!.localDate).toBe("2024-06-30");
    expect(berlin[0]!.localDate).toBe("2024-07-01");
    expect(clock(german[0]!.time)).toEqual([22, 30]);
    expect(clock(berlin[0]!.time)).toEqual([4, 30]);
  });

  it("handles the epoch and the supported final UTC instant without rounding", async () => {
    const result = await localizeTimestamps({
      timestampsMs: [0n, 253402300799999n],
      languageTag: "en-GB",
      timeZone: "UTC",
    });
    expect(result.timestamps.map((value) => value.localDate)).toEqual([
      "1970-01-01",
      "9999-12-31",
    ]);
    const [epochWest] = await localized(["1970-01-01T00:00:00Z"]);
    expect(epochWest!.localDate).toBe("1969-12-31");
  });

  it("returns errors instead of substituting UTC or formatting an invalid instant", async () => {
    const valid = { timestampsMs: [0n], languageTag: "en-GB", timeZone: "UTC" };
    for (const request of [
      { ...valid, timeZone: "Not/AZone" },
      { ...valid, timeZone: "" },
      { ...valid, languageTag: "" },
      { ...valid, languageTag: "not_a_locale" },
      { ...valid, timestampsMs: [-1n] },
      { ...valid, timestampsMs: [253402300800000n] },
      { ...valid, timestampsMs: Array<bigint>(129).fill(0n) },
      { ...valid, timestampsMs: [253402300799999n], timeZone: "Pacific/Kiritimati" },
    ]) {
      await expect(localizeTimestamps(request)).rejects.toBeInstanceOf(RangeError);
    }
  });
});
