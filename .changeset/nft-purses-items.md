---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.createItems` and `NftPurse.queryPurse`. `createItems` derives 1 to 1000 receiving keys in a purse, on the hard path `//nft//<purse>//<installation>//<index>` under the installation the host reports through the new `currentInstallation` host callback. `queryPurse` pages through the keys that hold an NFT on Asset Hub at the best block, at most 100 at a time. Another owner's purse needs the `NftPurseRead` grant for both. The browser core answers both `Unsupported`.
