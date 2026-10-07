// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

export const toHex = (bytes: Uint8Array): string =>
  `0x${Buffer.from(bytes).toString("hex")}`;

export const fromHex = (hex: string): Uint8Array =>
  new Uint8Array(Buffer.from(hex.replace(/^0x/, ""), "hex"));

/** Byte length of a hex string, with or without its `0x` prefix. */
export const hexByteLength = (hex: string): number =>
  hex.replace(/^0x/, "").length / 2;

/** Shorten a key for display, e.g. `0x84ccf320…be6232`. Recognizable without clutter. */
export const shortHex = (value: Uint8Array | string): string => {
  const hex = typeof value === "string" ? value : toHex(value);
  return hex.length <= 14 ? hex : `${hex.slice(0, 10)}…${hex.slice(-6)}`;
};
