/**
 * Print the host-papp encoding of every SSO message the core pins in
 * `rust/crates/truapi/src/host_internal/sso_messages.rs`
 * (`assert_host_papp_fixture`).
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
const seq = (start: number, n: number) => '0x' + Array.from({ length: n }, (_, i) => ((start + i) & 0xff).toString(16).padStart(2, '0')).join('');
const bytes = (h: string) => new Uint8Array(Buffer.from(h.replace(/^0x/, ''), 'hex'));

const productTx = (messageId: string, genesisHash: string, extensions: any[]) => ({
  messageId,
  data: { tag: 'v1', value: { tag: 'CreateTransactionRequest', value: { payload: { tag: 'v1', value: {
    signer: ['truapi-playground.dot', { tag: 'Index', value: 0 }],
    genesisHash, callData: new Uint8Array([0, 0]), extensions, txExtVersion: 0,
  } } } } },
});

const checkNonce = [{ id: 'CheckNonce', extra: new Uint8Array([1]), additionalSigned: new Uint8Array([2, 3]) }];
const messages: Record<string, any> = {
  product_tx: productTx('m-product-tx', seq(32, 32), checkNonce),
  playground_tx: productTx('create-transaction-1', '0xbf0488dbe9daa1de1c08c5f743e26fdc2a4ecd74cf87dd1b4b1eeb99ae4ef19f', []),
  legacy_tx: {
    messageId: 'm-legacy-tx',
    data: { tag: 'v1', value: { tag: 'CreateTransactionLegacyRequest', value: { payload: { tag: 'v1', value: {
      signer: bytes(seq(0, 32)), genesisHash: seq(32, 32), callData: new Uint8Array([0, 0]), extensions: checkNonce, txExtVersion: 0,
    } } } } },
  },
  resource_allocation: {
    messageId: 'm-resource',
    data: { tag: 'v1', value: { tag: 'ResourceAllocationRequest', value: {
      callingProductId: 'truapi-playground.dot',
      resources: [
        { tag: 'StatementStoreAllowance', value: undefined },
        { tag: 'BulletInAllowance', value: undefined },
        { tag: 'SmartContractAllowance', value: { tag: 'Index', value: 9 } },
        { tag: 'AutoSigning', value: undefined },
      ],
      onExisting: 'Increase',
    } } },
  },
  sign_raw_bytes: {
    messageId: 'm-legacy-raw',
    data: { tag: 'v1', value: { tag: 'SignRawLegacyRequest', value: {
      account: bytes(seq(0, 32)), data: { tag: 'Bytes', value: new Uint8Array(Buffer.from('Hi')) },
    } } },
  },
  sign_raw_payload: {
    messageId: 'm-legacy-raw-payload',
    data: { tag: 'v1', value: { tag: 'SignRawLegacyRequest', value: {
      account: bytes(seq(0, 32)), data: { tag: 'Payload', value: '<Bytes>Hi</Bytes>' },
    } } },
  },
  ring_vrf_alias: {
    messageId: 'm-alias',
    data: { tag: 'v1', value: { tag: 'RingVrfAliasRequest', value: {
      callingProductId: 'caller.dot',
      keyHandle: ['peopl.dot', { tag: 'Index', value: 0 }],
      context: ['voting.dot', { tag: 'Index', value: 0 }],
      ring: { chainId: '0x' + '11'.repeat(32), junctions: [
        { tag: 'PalletInstance', value: 67 },
        { tag: 'CollectionId', value: new Uint8Array(Buffer.from('pop')) },
      ] },
    } } },
  },
  ring_vrf_proof: {
    messageId: 'm-proof',
    data: { tag: 'v1', value: { tag: 'RingVrfProofRequest', value: {
      callingProductId: 'caller.dot',
      keyHandle: ['peopl.dot', { tag: 'Index', value: 0 }],
      context: ['voting.dot', { tag: 'Index', value: 0 }],
      ring: { chainId: '0x' + '11'.repeat(32), junctions: [
        { tag: 'PalletInstance', value: 67 },
        { tag: 'CollectionId', value: new Uint8Array(Buffer.from('pop')) },
      ] },
      message: new Uint8Array(Buffer.from('vote')),
    } } },
  },
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
