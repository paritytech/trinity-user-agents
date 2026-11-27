---
"@parity/truapi-host": patch
---

A signing host records in core storage which Statement Store slot and Bulletin authorization its wallet confirmed on chain, so after a restart it serves those allowance keys without reading the chain again. The records are two new `CoreStorageKey` kinds, `ConfirmedStatementAllowance` and `ConfirmedBulletinAllowance`, each owned by its product. A Statement Store record holds for its allowance period or until the chain refuses the key, and a Bulletin record until a refused submission asks for more. No key is stored: the wallet derives it. A host stores them like any other core storage value, and the wire protocol is unchanged.
