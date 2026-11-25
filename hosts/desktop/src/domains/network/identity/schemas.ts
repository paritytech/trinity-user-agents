import { Bytes, Enum } from '@novasamatech/scale';
import { Struct } from 'scale-ts';
import * as v from 'valibot';

import { hexString } from '@/shared/types';

/**
 * `Resources.Consumers.identifier_key` — a peer's chat encryption key as the chain
 * records it (CHAT-RFC-0004 §4).
 *
 * The 65-byte width predates X25519: it is what an uncompressed P-256 point
 * occupied, and it stayed when the curve changed. Only the keypair type and the key
 * width moved (0x04 + 64 → 0x00 + 32), so the field is still `SizedHex<65>` in
 * runtime metadata — and papi decodes a fixed-width field to a hex string, which is
 * what `identifier_key` below expects. Variable-width fields still arrive as bytes.
 *
 * Padding is a field rather than a skip because the RFC requires readers to ignore
 * it, not validate it. Decoding throws on a keypair type this host does not
 * implement; the service maps that to `null`.
 */
export const identifierKeyCodec = Enum({
  X25519: Struct({ key: Bytes(32), padding: Bytes(32) }),
});

/**
 * The parts of a `Resources.Consumers` row this host reads.
 *
 * Deliberately loose: extra runtime fields are ignored, and each optional field
 * degrades on its own rather than discarding the whole record.
 */
export const consumerRowSchema = v.object({
  lite_username: v.instance(Uint8Array),
  full_username: v.optional(v.nullable(v.instance(Uint8Array))),
  identifier_key: v.optional(v.nullable(hexString)),
  credibility: v.optional(
    v.nullable(
      v.object({
        type: v.string(),
        value: v.optional(v.nullable(v.object({ alias: v.string(), last_update: v.optional(v.nullable(v.bigint())) }))),
      }),
    ),
  ),
});
