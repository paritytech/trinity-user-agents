package io.paritytech.polkadotapp.chains.storage.source.query

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.storage.StorageEntry

class WithRawValue<T>(val at: BlockHash?, val raw: StorageEntry, val chainId: ChainId, val value: T)

data class AtBlock<T>(val value: T, val at: BlockHash)

fun <T> WithRawValue<T>.toAtBlock(): AtBlock<T> {
    return AtBlock(
        at = requireNotNull(at) { "Block hash was not specified" },
        value = value
    )
}
