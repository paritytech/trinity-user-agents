import { toHex } from '@novasamatech/host-api';

export const STATEMENT_TOPIC_BYTES = 32;

// Statement-store topics are fixed-size 32-byte hashes; reject malformed input before it reaches native.
export function validateStatementTopics(topics: Uint8Array[]): string | null {
  const invalid = topics.find((topic) => topic.length !== STATEMENT_TOPIC_BYTES);
  if (invalid === undefined) return null;

  const asUtf8 = new TextDecoder().decode(invalid);
  return `Statement topic must be ${STATEMENT_TOPIC_BYTES} bytes, got ${invalid.length}: ${toHex(invalid)} ("${asUtf8}")`;
}
