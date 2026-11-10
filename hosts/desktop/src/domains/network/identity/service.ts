import { toHex } from '@novasamatech/scale';
import * as v from 'valibot';

import { type HexString, hexString } from '@/shared/types';

import { consumerRowSchema, identifierKeyCodec } from './schemas';
import { type ConsumerIdentity, type Credibility } from './types';

const textDecoder = new TextDecoder();

function toCredibility(raw: v.InferOutput<typeof consumerRowSchema>['credibility']): Credibility {
  if (raw?.type !== 'Person' || !raw.value) return { type: 'Lite' };

  // The alias is chain-sourced text; a value that is not hex degrades the record to
  // `Lite` rather than surfacing a malformed alias to callers.
  const alias = v.safeParse(hexString, raw.value.alias);
  if (!alias.success) return { type: 'Lite' };

  return {
    type: 'Person',
    alias: alias.output,
    lastUpdate: raw.value.last_update?.toString() ?? null,
  };
}

/**
 * A `Resources.Consumers` row in this domain's vocabulary, or `null` when the
 * account holds no record.
 *
 * Every field degrades on its own. A keypair type this host cannot encrypt to
 * yields `identifierKey: null` rather than discarding the username, because an
 * account without chat is still an account worth naming.
 */
function toConsumerIdentity(accountId: string, raw: unknown): ConsumerIdentity | null {
  const parsed = v.safeParse(consumerRowSchema, raw);
  if (!parsed.success) return null;

  const row = parsed.output;

  return {
    accountId,
    fullUsername: row.full_username ? textDecoder.decode(row.full_username) : null,
    liteUsername: textDecoder.decode(row.lite_username),
    credibility: toCredibility(row.credibility),
    identifierKey: row.identifier_key ? decodeIdentifierKey(row.identifier_key) : null,
  };
}

// The chain hands this field over as hex (see `schemas.ts`); scale decoders take that
// form directly, so there is nothing to convert first.
function decodeIdentifierKey(encoded: HexString): HexString | null {
  try {
    return toHex(identifierKeyCodec.dec(encoded).value.key);
  } catch {
    // A keypair variant this host does not implement — expected, not a fault.
    return null;
  }
}

export const consumerIdentityService = {
  toConsumerIdentity,
};
