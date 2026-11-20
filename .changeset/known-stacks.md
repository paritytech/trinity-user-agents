---
"@parity/trinity-stack": minor
---

The package and the crate list the links, signed or unsigned, of the known Trinity stacks: paseo-next-v2, pcf-devnet,
polkadot, previewnet and westend. `knownStacks` maps each name to its link, and the crate's `known_stack` and
`known_stack_links` return the same. `trinity-stack add <name>` without a link adds the signed known stack of that name,
and `trinity-stack known` prints each name and link and marks the unsigned ones, which are listed for their link only.
