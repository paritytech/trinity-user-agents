// A ring-VRF proof in a third product's context, for
// paritytech/platform-bugs#99. Run by `scripts/cross-product-ringvrf-context-e2e.sh`,
// once per phase, with the grants from local product configs:
//
//   peopl.paseo  grants dim2next `context`: the key grant.
//   dim2.paseo   grants dim2next `context`: the third product consents.
//   stash.paseo  grants nothing.
//
// An admitted proof answers `NotMember`: the context passed, and the test
// signer is not a person. A refusal is `NotAllowlisted` for a proof and
// `Rejected` for an alias.

const index = { tag: "Index" as const, value: 0 };
const keyHandle = { dotNsIdentifier: "peopl.paseo", derivationIndex: index };

const people = await truapi.chain.getChainInfo({ chain: "People" });
assert(people.isOk(), "get_chain_info failed", people);
const ringLocation = {
  chainId: people.value.genesisHash,
  junctions: [
    {
      tag: "CollectionId" as const,
      // "pop:polkadot.network/people-lite"
      value:
        "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465",
    },
  ],
};

/// `Ok`, or the error variant.
const outcome = (result: {
  isOk(): boolean;
  error?: { value?: { value?: { tag?: string } } };
}) => (result.isOk() ? "Ok" : result.error?.value?.value?.tag);

if (process.env.E2E_PHASE === "register") {
  const key = await truapi.account.registerRingVrfKey({
    index,
    ring: ringLocation,
  });
  assert(key.isOk(), "register_ring_vrf_key failed", key);
  console.log("registered peopl.paseo's ring-VRF key");
} else {
  for (const [productId, proof, alias] of [
    ["dim2.paseo", "NotMember", "Ok"],
    ["stash.paseo", "NotAllowlisted", "Rejected"],
  ]) {
    const context = { productId, suffix: index };
    const proved = outcome(
      await truapi.account.createAccountProof({
        keyHandle,
        context,
        ringLocation,
        message: "0x7475726e",
      }),
    );
    const read = outcome(
      await truapi.account.getAccountAlias({
        keyHandle,
        context,
        ringLocation,
      }),
    );
    console.log(`${productId}: proof ${proved}, alias ${read}`);
    assert(proved === proof, `proof in ${productId}: expected ${proof}`);
    assert(read === alias, `alias in ${productId}: expected ${alias}`);
  }
}

export {};
