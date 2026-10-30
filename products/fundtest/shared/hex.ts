// Hex as the host's APIs take bytes: lowercase and 0x-prefixed.

/** Lowercase hex of `bytes`, 0x-prefixed. */
export const toHex = (bytes: Uint8Array): `0x${string}` =>
  `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;

/** The bytes of a 0x-prefixed hex string. */
export const fromHex = (hex: string): Uint8Array =>
  Uint8Array.from(hex.slice(2).match(/.{2}/g) ?? [], (pair) =>
    Number.parseInt(pair, 16),
  );
