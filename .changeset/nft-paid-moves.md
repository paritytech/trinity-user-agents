---
"@parity/truapi": major
"@parity/truapi-host": patch
---

NFT purses keep moving items after their fee-less moves run out. Native signing hosts record how many fee-less moves each held NFT used, and a transfer, rebalance or refund of an NFT with none left is paid: the host claims PGAS into the item's key and moves it with `Scarcity::transfer_by_holder`, whose fee the runtime takes from that PGAS. `createCheque` answers the new `NftPurseCreateChequeError::ItemsMovesExhausted`, naming every item with no fee-less move left, so the product rebalances them first. Products built against the previous error enum cannot decode the new variant.
