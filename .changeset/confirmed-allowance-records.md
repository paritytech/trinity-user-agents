---
"@parity/truapi-host": patch
---

A native signing host records in the core database which Statement Store slot and Bulletin authorization its wallet confirmed on chain, so after a restart it serves those allowance keys without reading the chain again. A Statement Store record holds for its allowance period or until the chain refuses the key, and a Bulletin record until a refused submission asks for more. No key is stored: the wallet derives it. The JavaScript API and wire protocol are unchanged.
