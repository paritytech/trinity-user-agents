/** Seconds since the epoch; an invalid `Date` would otherwise turn off staleness and expiry. */
export function unixSeconds(date: Date): number {
  const milliseconds = date.getTime();
  if (Number.isNaN(milliseconds))
    throw new RangeError("now is an invalid Date");
  return Math.floor(milliseconds / 1000);
}
