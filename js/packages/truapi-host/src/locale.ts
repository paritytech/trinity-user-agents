import type { LocaleHost } from "./generated/host-callbacks.js";

/** Localize each instant using the requested zone's historical offset and DST. */
export const localizeTimestamps: NonNullable<LocaleHost["localizeTimestamps"]> = async (
  request,
) => {
  if (!request.languageTag.trim() || !request.timeZone.trim()) {
    throw new RangeError("A language tag and time zone are required");
  }
  if (
    request.timestampsMs.length > 128 ||
    request.timestampsMs.some(
      (timestamp) => timestamp < 0n || timestamp > 253402300799999n,
    )
  ) {
    throw new RangeError("Timestamp batch or instant is out of range");
  }
  const { languageTag, timeZone } = request;
  const localDate = new Intl.DateTimeFormat("en-US", {
    calendar: "gregory",
    numberingSystem: "latn",
    timeZone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  const time = new Intl.DateTimeFormat(languageTag, {
    timeZone,
    hour: "numeric",
    minute: "2-digit",
  });
  const date = new Intl.DateTimeFormat(languageTag, {
    timeZone,
    dateStyle: "long",
  });
  const dateTime = new Intl.DateTimeFormat(languageTag, {
    timeZone,
    dateStyle: "full",
    timeStyle: "long",
  });
  return {
    timestamps: request.timestampsMs.map((timestamp) => {
      const instant = Number(timestamp);
      const parts = localDate.formatToParts(instant);
      const year = parts.find((part) => part.type === "year")!.value;
      const month = parts.find((part) => part.type === "month")!.value;
      const day = parts.find((part) => part.type === "day")!.value;
      if (year.length > 4) {
        throw new RangeError("Local date is outside the four-digit year range");
      }
      return {
        localDate: `${year.padStart(4, "0")}-${month}-${day}`,
        time: time.format(instant),
        date: date.format(instant),
        dateTime: dateTime.format(instant),
      };
    }),
  };
};
