---
"@parity/truapi": major
---

Three envelope codecs take the name of the payload they carry: `VersionedHostCreateTransactionRequest` is now `VersionedProductAccountTxPayload`, `VersionedHostCreateTransactionWithLegacyAccountRequest` is `VersionedLegacyAccountTxPayload`, and `VersionedRemotePreimageSubmitError` is `VersionedPreimageSubmitError`. Code that names one of these types, such as the error of `preimage.submit`, uses the new name. The bytes on the wire are unchanged. `TRUAPI_WIRE_SCHEMA_HASH` changes with the names, so a debugger built before this release does not decode frames from a host built after it.
