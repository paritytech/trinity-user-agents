---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.deposit` and the new `NftPurse.staticReceiveKey`. `deposit` opens a received cheque and registers one move per item into the purse the product names, under the cheque id, which `observeOperationStatus` follows. An `Ephemeral` cheque may be deposited only by the product that made its receive key, and only into that key's purse: another purse answers the new `NftPurseDepositError.PurseMismatch`, and another product's receive key answers `CorruptedCheque`. A `Static` cheque opens with the key `staticReceiveKey` returns, the same for every product, and any product may deposit it into any purse. Each move waits up to an hour for its cheque key to hold an NFT. The item secrets are kept in the host's encrypted storage, so each move can be signed again after a restart. The browser core answers both calls `Unsupported`.
