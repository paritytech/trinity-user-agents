import { pbkdf2 } from "@noble/hashes/pbkdf2.js";
import { sha512 } from "@noble/hashes/sha2.js";
import * as sr25519 from "@scure/sr25519";
import { concat, text, u64, utf8 } from "./codec.js";
import {
  blake2b256,
  keyedBlake2b256,
  x25519Agree,
  x25519PublicKey,
} from "./crypto.js";

/**
 * The two keys a wallet chats with, derived from its BIP-39 entropy the way
 * the core and the Polkadot app derive them:
 *
 * - the identity account, sr25519 at `//product//uid.<suffix>/index(0)` from
 *   the Substrate mini-secret of the entropy. Requests are addressed to it, it
 *   names this side of every session topic, and it signs every statement;
 * - the chat key, X25519 from `blake2b(key: "//chat" chain code,
 *   blake2b(key: "ecdh", entropy))`. Peers find it on chain and agree every
 *   shared secret with it.
 *
 * The secrets stay in this object. Callers get public keys, signatures and
 * agreed secrets.
 */
export class ChatKeys {
  readonly identity: Uint8Array;
  readonly chatPublic: Uint8Array;
  readonly #identitySecret: Uint8Array;
  readonly #chatSecret: Uint8Array;

  private constructor(identitySecret: Uint8Array, chatSecret: Uint8Array) {
    this.#identitySecret = identitySecret;
    this.#chatSecret = chatSecret;
    this.identity = sr25519.getPublicKey(identitySecret);
    this.chatPublic = x25519PublicKey(chatSecret);
  }

  static fromEntropy(entropy: Uint8Array, networkSuffix: string): ChatKeys {
    return new ChatKeys(
      deriveIdentitySecret(entropy, networkSuffix),
      deriveChatSecret(entropy),
    );
  }

  sign(message: Uint8Array): Uint8Array {
    return sr25519.sign(this.#identitySecret, message);
  }

  /** The raw X25519 secret shared with a peer's chat key. */
  agree(peerChatKey: Uint8Array): Uint8Array {
    return x25519Agree(this.#chatSecret, peerChatKey);
  }
}

const JUNCTION_BYTES = 32;

/** A Substrate junction's chain code: its SCALE encoding padded to 32 bytes, or hashed when longer. */
export function chainCode(encoded: Uint8Array): Uint8Array {
  if (encoded.length > JUNCTION_BYTES) return blake2b256(encoded);
  const code = new Uint8Array(JUNCTION_BYTES);
  code.set(encoded);
  return code;
}

/** The chain code of a `//name` junction: digits encode as a u64, anything else as a string. */
export function junctionCode(junction: string): Uint8Array {
  return chainCode(
    /^\d+$/.test(junction) ? u64(BigInt(junction)) : text(junction),
  );
}

/** The X25519 chat secret: the `chat` domain under the entropy's `ecdh` root. */
export function deriveChatSecret(entropy: Uint8Array): Uint8Array {
  const root = keyedBlake2b256(utf8("ecdh"), entropy);
  return keyedBlake2b256(chainCode(text("chat")), root);
}

/** `index_bytes(0)`: a zero u32 and the first 28 bytes of `blake2b256("product-account-index")`. */
export function identityIndex(): Uint8Array {
  const magic = blake2b256(utf8("product-account-index")).subarray(0, 28);
  return concat(new Uint8Array(4), magic);
}

/** The Substrate mini-secret of BIP-39 entropy with no password. */
export function miniSecret(entropy: Uint8Array): Uint8Array {
  return pbkdf2(sha512, entropy, utf8("mnemonic"), {
    c: 2048,
    dkLen: 64,
  }).slice(0, 32);
}

/** The sr25519 secret of the identity account on the network with `networkSuffix`. */
export function deriveIdentitySecret(
  entropy: Uint8Array,
  networkSuffix: string,
): Uint8Array {
  let secret: Uint8Array = sr25519.secretFromSeed(miniSecret(entropy));
  for (const junction of ["product", `uid.${networkSuffix}`])
    secret = sr25519.HDKD.secretHard(secret, junctionCode(junction));
  return sr25519.HDKD.secretSoft(secret, identityIndex());
}
