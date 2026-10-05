---
"@parity/truapi": patch
---

The core has shared chain capabilities for host services: `ChainHeads` (finalized and best heads, head events), `BlockBackend` (block hash by number, block number, extrinsic hashes, dispatch outcome), `TxValidator` and `TxSubmitter`, implemented directly by the runtime's chain connections (`ChainRuntime`). Block reads and validation go through a subxt client on the legacy JSON-RPC methods, so they reach blocks a chainHead follow no longer pins, and validation runs against the best block as the transaction pool does. Submission goes through the existing chainHead client. A node answer that cannot prove absence, such as a pruned body, is an error rather than "not found", and a closed connection is reported as unavailable. Both clients on a connection share one chain config, so runtime metadata is downloaded once per spec version.
