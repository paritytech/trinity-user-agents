package io.paritytech.polkadotapp.tools_ipfs_api

object Cids {
    fun castCatching(bytes: ByteArray): Result<Cid> {
        return runCatching { Cid.cast(bytes) }
    }
}
