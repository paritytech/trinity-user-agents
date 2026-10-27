---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

The core builds its own extrinsics through one builder: statement-store and long-term-storage allowances, PGAS claims, Bulletin preimages and the CLI's dotNS registrations. Every extrinsic is mortal, born at the finalized head for 64 blocks, and signs the genesis the host configured. Its version follows the new `transactionExtensionVersions` host callback, in the `extrinsicVersion` group: the chains the host builds as V5 general transactions and the transaction extension pipeline each uses; every other chain is V4. A web host implementing `RequiredHostCallbacks` has to answer it; an empty list builds every chain as V4, which leaves the core unable to build the allowance, PGAS and dotNS extrinsics, since those exist only as V5. The `truapi-host` CLI answers People and Asset Hub on pipeline 1.
