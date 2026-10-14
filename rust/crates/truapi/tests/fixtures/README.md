# Test fixtures

Frozen chain data for the offline unit tests. Nothing here is loaded at runtime; every
file is reached through `include_bytes!` from a `#[cfg(test)]` path, so none of it reaches
the wasm bundle.

## Metadata

Raw `RuntimeMetadataPrefixed`, which is what `statement_allowance::extension::Metadata::decode`
expects.

| File | Chain | Metadata | Spec | Captured | Declares |
|---|---|---|---|---|---|
| `paseo-next-v2-metadata.scale` | Paseo Next v2 | V14 | | | `AsResources`, three-field allowance info |
| `paseo-next-v2-metadata-v16.scale` | Paseo Next v2 | V16 | 3000000 | 2026-09-03 | `AsResources`, four-field allowance info; `Resources` slot budgets as view functions |
| `paseo-next-asset-hub-metadata.scale` | Paseo Asset Hub Next | V16 | 3000000 | 2026-09-03 | `AsPgas`, `Pgas`, `MembersSubscriber` incl. `CurrentGeneration` |
| `bulletin_paseo_metadata.scale` | Polkadot Bulletin (Paseo) | V14 | 1000020 | | preimage and storage calls |

The two paseo-next-v2 fixtures deliberately disagree about arity: V14 predates the
`revision` field, and `the_two_fixtures_disagree_about_the_allowance_arity` pins that.

Capture with the subxt CLI:

```bash
subxt metadata --url wss://<endpoint> --version 16 -f bytes > <name>.scale
```

Read the spec version from `state_getRuntimeVersion` at the same time and record it above.
Blank cells are fixtures that predate this README.

## Storage values

Storage values are decoded against the metadata fixture's own type registry, so a storage
fixture and the metadata beside it are a matched pair. Capture both from the same runtime,
and replace both together. Re-capturing metadata alone will fail
`captured_ring_roots_project_to_their_revisions` if the record layout changed.

| File | Storage | Chain | Generation | Block | Captured |
|---|---|---|---|---|---|
| `paseo-next-asset-hub-ring-5-roots.scale` | `MembersSubscriber.RingRoots[(generation, LitePeople, 5)]` | Paseo Asset Hub Next | 0 | `0xf25d4e330ade1ce230695976f019df50cdaf97c96b6996838af93b68550654f3` | 2026-08-17 |

Ring 5 holds `[105, 106, 108]`. The skipped 107 is the case that distinguishes testing the
newest held root from testing the oldest, and freezing it makes that case permanent
instead of dependent on a chain window that moves. Of the fourteen lite-people rings
holding roots at that block, it was the only one that was not contiguous.

The generation column is what the key's first term has to be to address this value.
It is 0 because the block predates the generation term, so at that block the entry was
addressed by the two remaining keys, and `MembersSubscriber.CurrentGeneration` is still
unset on paseo Asset Hub Next, which reads as 0 through its `ValueQuery` default. A
capture taken after the first rebuild has to record the generation it was read under, or
the key recipe below will not reach it. The offline tests build the key from a synthetic
generation and assert it separately, so they do not depend on this value.

There is no CLI for a storage read by raw key. Build the key the way `pgas::ring_roots_key`
does, then call `state_getStorageAt`:

```
twox_128("MembersSubscriber")
  ‖ twox_128("RingRoots")
  ‖ twox_64_concat(current_generation_u32_le)
  ‖ blake2_128_concat(b"pop:polkadot.network/people-lite")
  ‖ blake2_128_concat(ring_index_u32_le)
```

```bash
curl -s -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"state_getStorageAt","params":["0x<key>","0x<block>"]}' \
  https://paseo-asset-hub-next-rpc.polkadot.io
```

Read `MembersSubscriber.CurrentGeneration` first. The generation uses `Twox64Concat`; the
collection and ring index use `Blake2_128Concat`.

## Recapturing

A re-captured fixture can change shape silently. Two tests exist to catch that rather than
letting it surface as a runtime panic inside `validate_transaction`:
`the_asset_hub_fixture_declares_the_pgas_claim_shape` pins the `AsPgas::Claim` arity, and
`the_captured_asset_hub_artefacts_load` pins the metadata version. Update the table above
whenever a file is replaced.
