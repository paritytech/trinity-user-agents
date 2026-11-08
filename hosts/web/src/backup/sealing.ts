import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import { hexToBytes, type HexString } from "@parity/truapi/scale";
import { concat, equalBytes, utf8 } from "../platform/codec.js";
import { cryptoRandom, type RandomBytes } from "../platform/crypto.js";

/**
 * The backup's keys and envelopes. Every key is derived from the wallet's
 * entropy for this purpose alone, under {@link DOMAIN}, and bound to the
 * People chain and the identity it backs up, so another network's or another
 * person's backup never opens with them. None is a chat or an account key.
 *
 * An envelope is a magic, a random 24-byte nonce and XChaCha20-Poly1305
 * ciphertext, with the magic, the binding and the purpose as associated
 * data. The random nonce also makes each snapshot's bytes, and so its
 * Bulletin key, impossible to predict.
 */

const DOMAIN = "truapi-web-host/backup/v1";
const NONCE_BYTES = 24;

/**
 * The bytes a pointer's data starts with. The core's backup statement
 * account signs nothing else (`BACKUP_POINTER_MAGIC`).
 */
export const POINTER_MAGIC = utf8("twhb/ptr/1");
const SNAPSHOT_MAGIC = utf8("twhb/snap/1");

/** What a backup is of: one identity on one network. */
export interface BackupBinding {
  network: string;
  /** The People chain's genesis hash. */
  peopleGenesis: HexString;
  /** The identity account the wallet signs in as. */
  identity: HexString;
}

export interface BackupKeys {
  snapshot: Uint8Array;
  pointer: Uint8Array;
  /** The Statement Store topic the pointer is found under. */
  topic: Uint8Array;
  /** The pointer's channel, so a new pointer replaces the last. */
  channel: Uint8Array;
}

/** The backup keys of the wallet with `entropy`, for `binding`. */
export function backupKeys(
  entropy: Uint8Array,
  binding: BackupBinding,
): BackupKeys {
  const salt = concat(
    hexToBytes(binding.peopleGenesis),
    hexToBytes(binding.identity),
  );
  const derive = (purpose: string) =>
    hkdf(sha256, entropy, salt, utf8(`${DOMAIN}/${purpose}`), 32);
  return {
    snapshot: derive("snapshot-key"),
    pointer: derive("pointer-key"),
    topic: derive("pointer-topic"),
    channel: derive("pointer-channel"),
  };
}

const associated = (
  magic: Uint8Array,
  binding: BackupBinding,
  extra: string,
): Uint8Array =>
  concat(
    magic,
    utf8(
      `|${binding.network}|${binding.peopleGenesis.toLowerCase()}|${binding.identity.toLowerCase()}|${extra}`,
    ),
  );

function seal(
  key: Uint8Array,
  magic: Uint8Array,
  aad: Uint8Array,
  plaintext: Uint8Array,
  random: RandomBytes,
): Uint8Array {
  const nonce = random(NONCE_BYTES);
  return concat(
    magic,
    nonce,
    xchacha20poly1305(key, nonce, aad).encrypt(plaintext),
  );
}

function open(
  key: Uint8Array,
  magic: Uint8Array,
  aad: Uint8Array,
  sealed: Uint8Array,
  what: string,
): Uint8Array {
  if (
    sealed.length < magic.length + NONCE_BYTES ||
    !equalBytes(sealed.subarray(0, magic.length), magic)
  )
    throw new Error(`not a backup ${what}`);
  const nonce = sealed.subarray(magic.length, magic.length + NONCE_BYTES);
  try {
    return xchacha20poly1305(key, nonce, aad).decrypt(
      sealed.subarray(magic.length + NONCE_BYTES),
    );
  } catch {
    throw new Error(
      `the backup ${what} does not open: it is another wallet's, network's or identity's, or it was changed`,
    );
  }
}

/** Seal a compressed snapshot for `binding`. */
export const sealSnapshot = (
  keys: BackupKeys,
  binding: BackupBinding,
  plaintext: Uint8Array,
  random: RandomBytes = cryptoRandom,
): Uint8Array =>
  seal(
    keys.snapshot,
    SNAPSHOT_MAGIC,
    associated(SNAPSHOT_MAGIC, binding, "snapshot"),
    plaintext,
    random,
  );

/** Open a sealed snapshot of `binding`; throws on any other. */
export const openSnapshot = (
  keys: BackupKeys,
  binding: BackupBinding,
  sealed: Uint8Array,
): Uint8Array =>
  open(
    keys.snapshot,
    SNAPSHOT_MAGIC,
    associated(SNAPSHOT_MAGIC, binding, "snapshot"),
    sealed,
    "snapshot",
  );

/** Seal a pointer's body for `binding`, on its channel. */
export const sealPointer = (
  keys: BackupKeys,
  binding: BackupBinding,
  plaintext: Uint8Array,
  random: RandomBytes = cryptoRandom,
): Uint8Array =>
  seal(
    keys.pointer,
    POINTER_MAGIC,
    associated(POINTER_MAGIC, binding, "pointer"),
    plaintext,
    random,
  );

/** Open a pointer's data sealed for `binding`; throws on any other. */
export const openPointer = (
  keys: BackupKeys,
  binding: BackupBinding,
  sealed: Uint8Array,
): Uint8Array =>
  open(
    keys.pointer,
    POINTER_MAGIC,
    associated(POINTER_MAGIC, binding, "pointer"),
    sealed,
    "pointer",
  );

/** The largest snapshot a restore decompresses. */
const MAX_UNPACKED_BYTES = 32 * 1024 * 1024;

/** `bytes` gzip-compressed. */
export async function compress(bytes: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([bytes as BlobPart])
    .stream()
    .pipeThrough(new CompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** `bytes` gzip-decompressed; throws past {@link MAX_UNPACKED_BYTES}. */
export async function decompress(bytes: Uint8Array): Promise<Uint8Array> {
  const reader = new Blob([bytes as BlobPart])
    .stream()
    .pipeThrough(new DecompressionStream("gzip"))
    .getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > MAX_UNPACKED_BYTES) {
      await reader.cancel();
      throw new Error("the backup's snapshot unpacks larger than a backup is");
    }
    chunks.push(value);
  }
  return concat(...chunks);
}
