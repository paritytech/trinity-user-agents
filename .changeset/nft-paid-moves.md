---
"@parity/truapi": major
"@parity/truapi-host": patch
---

NFT purses keep moving items after their fee-less moves run out. Native signing hosts count the fee-less moves each
held NFT has used. A `transferItem` or `rebalancePurse` move of an NFT with none left is paid, and so is a `refund`
that would leave the NFT with none at the cheque key, so the sender can still revoke the cheque for free. To pay, the
host claims PGAS into the item's key without asking, which spends one slot of the user's daily PGAS budget, and moves
the NFT with `Scarcity::transfer_by_holder`. The runtime takes the fee from that PGAS.

`createCheque` answers the new `NftPurseCreateChequeError::ItemsMovesExhausted`, naming every item with no fee-less
move left, so the product rebalances those items first. Products built against the previous error enum cannot decode
the new variant. Until the host has read an item's moves from the chain, `createCheque`, `transferItem`,
`rebalancePurse` and `refund` answer `ItemUnavailable` for it, and the product calls again.
