---
"@parity/truapi-host": minor
---

Native hosts recover NFT purses after a reinstall. While a session is active, the core reads the installations the seed
registered in the stack's `AccountDataStore` contract and walks their purses on Asset Hub together: the main purse, then
purse ids from 0 until 10 in a row are empty under every earlier installation, each purse until more than 1000 keys in a
row hold nothing. Keys that hold an NFT return under their purse, and a purse with no record is recreated, owned by Root and named
`Recovered purse <id>`. Creating a purse waits while any registered installation is unfinished. Hosts change
nothing.
