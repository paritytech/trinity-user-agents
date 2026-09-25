/**
 * Print the host-papp encoding of every SSO message the core pins in
 * `rust/crates/truapi/src/host_internal/sso_messages.rs`
 * (`assert_host_papp_fixture`). Product requests carry the caller's
 * `ProductContext`, which host-papp does not encode, so only responses are
 * pinned against it.
 *
 * Runs host-papp's own codec from a triangle-js-sdks checkout, so a fixture is
 * what host-papp puts on the wire rather than what the core expects:
 *
 *   bun --conditions '#/source' scripts/host-papp-fixtures.ts <triangle-js-sdks>
 *
 * The checkout needs its dependencies installed.
 */
const root = process.argv[2];
if (!root) throw new Error('usage: host-papp-fixtures.ts <triangle-js-sdks checkout>');
const { RemoteMessageCodec } = await import(`${root}/packages/host-papp/src/sso/sessionManager/scale/remoteMessage.ts`);

const hex = (b: Uint8Array) => '0x' + Buffer.from(b).toString('hex');
const bytes = (h: string) => new Uint8Array(Buffer.from(h.replace(/^0x/, ''), 'hex'));

const messages: Record<string, any> = {
  ring_vrf_alias_response: {
    messageId: 'r-alias',
    data: { tag: 'v1', value: { tag: 'RingVrfAliasResponse', value: {
      respondingTo: 'm-alias',
      payload: { success: true, value: { context: bytes('22'.repeat(32)), alias: new Uint8Array([0x33, 0x44]) } },
    } } },
  },
  ring_vrf_proof_response: {
    messageId: 'r-proof',
    data: { tag: 'v1', value: { tag: 'RingVrfProofResponse', value: {
      respondingTo: 'm-proof',
      payload: { success: true, value: {
        proof: new Uint8Array([0x55, 0x66]),
        contextualAlias: { context: bytes('22'.repeat(32)), alias: new Uint8Array([0x33, 0x44]) },
        ringIndex: 7, ringRevision: 9,
      } },
    } } },
  },
};

for (const [name, message] of Object.entries(messages)) {
  console.log(`${name} ${hex(RemoteMessageCodec.enc(message))}`);
}
