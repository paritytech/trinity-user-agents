---
"@parity/truapi": patch
---

The CLI host signs lite username registrations in the form the network's runtime accepts. An individuality runtime that publishes a `SigningContext` constant on the verifying pallet (`PeopleLite` on People, `DotnsGateway` on Asset Hub) checks the candidate and consumer registration signatures and the dotNS reservation signature over the personhood signing context followed by the message, as paritytech/individuality-community#196 requires. The CLI reads the constant from each chain's metadata when it attests. A runtime without the constant keeps receiving signatures over the message alone, so the same release works before and after the runtime upgrade.
