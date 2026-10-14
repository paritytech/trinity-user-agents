# Revive (pallet-revive / EVM) Architecture

## Overview

`Packages/Revive/` owns everything the app knows about pallet-revive: the chain-bound contract API and
its one implementation, the runtime-API caller and result decoding, the `Revive.call` extrinsic, the EVM
address type with the H160 derivation, the ENS namehash, and the shared Solidity ABI encoder.

## Public surface

- `ReviveContractApiProtocol` — `callReadOnly(contract:input:at:)`, `dryRun(origin:contract:input:)`,
  `isAccountMapped(_:)`; `ReviveDryRun` (output, weight required, deposit that would be charged);
  `ReviveContractRevertedError` (the contract ran and reverted, `data` is its reason);
  `ReviveContractError` (`unexpectedRuntimeApiSignature`, `callFailed(JSON)` for a dispatch error).
- `ReviveContractApi(chainId:chainResource:operationQueue:readOnlyOrigin:)` — the implementation over
  `ChainStore.ChainResourceProtocol` (`ChainRegistry` conforms). Reads run as
  `ReviveReadOnlyCaller.accountId` (`modlpy/reviv`, zero-padded) so no mapping or balance is needed.
- `RevivePallet` — `name`, `callName`, `Call` (+ `CallArgs` / `LegacyCallArgs`), `WeightLimitArgument`,
  `weightLimitArgument(in:)`; `ReviveCallArgumentsProviding` + `RuntimeReviveCallArguments` read which
  name the runtime gives the weight limit (`weight_limit`, or `gas_limit` before the rename).
- `EvmAddress` (`= Data`) + `EvmAddressFormat` (`size`, `zero`, `validate(_:)`); `AccountId.toH160()`.
- `EvmAbi` — `Function(name:inputs:outputs:)`, `ParameterType`, `encode(_:parameters:)`,
  `decode(_:output:)`, `EvmAbiError`. The contract ABI tables (which functions, which argument types)
  stay with the feature that owns the contract (`AccountDataStoreAbi`, `DotNsAbi`); only the encoder is
  shared, and web3swift is linked by this package alone.
- `NameHash.nameHash(_:)`.

Internal: `ReviveContractCalling` / `ReviveContractCaller` (runtime API over
`SubstrateRuntimeApiOperationFactory`, storage read over `StorageRequestFactory`), the `ContractResult`
decode models (`ReviveDryRunResult`, `ReviveStorageDeposit`, `ReviveExecResult`, `ReviveReturnFlags`).

## Rules

1. **Encode `ReviveApi_call` parameters by name.** The caller looks each argument up in the runtime's
   declared inputs (`origin`, `dest`, `value`, `weight_limit` | `gas_limit`, `storage_deposit_limit`,
   `input_data`) and encodes it under that input's type. A runtime missing a name fails with
   `unexpectedRuntimeApiSignature` instead of being encoded by position.
2. **A revert is an error.** Both the read-only call and the dry run throw `ReviveContractRevertedError`
   when the return flags carry the revert bit; a dispatch error (the pallet refused the call) is
   `ReviveContractError.callFailed`. Callers never see revert bytes as content. DotNs wraps either as
   `DotNsContractError.contractCallFailed`, with one exception: a revert carrying no data is Solidity's
   bare `revert()` on a selector the contract does not implement (a registry entry can name a contract
   that is not a resolver), so `ReviveDotNsContractApi` reads it as "no record" and falls back exactly
   as it does for empty output.
3. **`EvmAddress` at every contract seam.** Contract addresses, `Revive.call`'s `dest`, and H160s are
   `EvmAddress`. Because it is an alias, bytes that come from outside pass `EvmAddressFormat.validate`
   at the boundary: the remote-config readers (`FirebaseApplicationService`, `AppConfig.DotNs.config()`)
   and `DotNsAbi.decodeResolver`. `RemoteAppConfig` keeps `Data?` only because the notification
   extension compiles it without linking `Revive`; its value is validated before it is stored.
4. **Both pallet-revive namings are live.** `Revive.call` takes `weight_limit` or `gas_limit`
   (`RevivePallet.weightLimitArgument(in:)`), and the dry run reports `weight_required` or `gas_required`
   (`ReviveDryRunResult`). Read the runtime; never assume one.
5. **Runtime literals in one place.** `"Revive"` is `RevivePallet.name`; the address size is
   `EvmAddressFormat.size`.

## Tests

`Packages/Revive/Tests/ReviveTests/` (in the `polkadot-app` test plan): named parameter encoding,
the `OriginalAccount` read path, `ContractResult` decoding under both spellings, revert / dispatch-error
bridging, `EvmAbi` vectors pinned against ethers, `EvmAddressFormat`, namehash vectors. Consumers keep
their contract tests (`AccountDataStoreAbiTests`, `DotNsAbiTests`, `ReviveDotNsContractApiTests`).

## Seams

| Seam                       | Where                                                    | When to touch                         |
|----------------------------|----------------------------------------------------------|---------------------------------------|
| Contract API               | `Packages/Revive/Sources/Revive/ReviveContractApi.swift` | New pallet-revive runtime API or storage read |
| Call encoding              | `ReviveContractCaller.encode(_:for:into:context:)`       | Runtime API signature changes          |
| `Revive.call` extrinsic    | `RevivePallet+Call.swift`                                | Call argument changes                  |
| ABI encoder                | `EvmAbi.swift`                                           | A Solidity type the tables need        |
| Address boundaries         | `EvmAddress.swift`, the three validating readers         | New source of contract addresses       |
| Wiring                     | `ServiceCoordinator+Coinage`, `SPAFlowStateProvider`     | Chain or queue for a consumer          |
