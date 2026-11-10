import * as v from 'valibot';

/** Expanded sr25519 secret, as `createSr25519Secret` produces it. */
const SEED_BYTES = 64;
/** X25519 scalar (CHAT-RFC-0004). */
const ENCRYPTION_KEY_BYTES = 32;

const bytes = (length: number) =>
  v.pipe(
    v.instance(Uint8Array),
    v.check(value => value.length === length, `expected ${length} bytes`),
  );

/**
 * The persisted device identity, validated on read.
 *
 * A row written by an older build — or a truncated key — must read as a miss rather
 * than reach `createSr25519Prover`, which would otherwise sign with a malformed
 * secret and produce statements no peer can verify.
 */
export const deviceIdentityRowSchema = v.object({
  id: v.string(),
  statementAccountSeed: bytes(SEED_BYTES),
  encryptionPrivateKey: bytes(ENCRYPTION_KEY_BYTES),
  createdAt: v.number(),
});
