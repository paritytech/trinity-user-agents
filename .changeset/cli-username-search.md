---
"@parity/truapi": patch
---

The `truapi-host signing-host` shell has a `/search <prefix>` command. It lists the usernames the network's identity backend holds under the prefix, at most 50, with what Asset Hub records for each: the `DotnsGateway` pallet's `LiteLabelOwner` and `AccountNames` entries, and the username the dotNS contracts resolve for the account. A name the backend assigned but the chain never recorded shows `none` in the chain columns.
