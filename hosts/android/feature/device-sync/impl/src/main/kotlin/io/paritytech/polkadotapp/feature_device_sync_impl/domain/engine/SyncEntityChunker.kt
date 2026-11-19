package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale.SyncEntityScale

/** Splits a snapshot into batches of at most [maxPayloadSize] encoded entities, keeping their order. */
class SyncEntityChunker(private val maxPayloadSize: InformationSize) {
    fun chunk(entities: List<SyncEntityScale>): List<List<SyncEntityScale>> {
        val pieces = entities.flatMap(::splitOversized)
        return greedyPack(pieces) { it.encodedSize() }
    }

    private fun splitOversized(entity: SyncEntityScale): List<SyncEntityScale> {
        if (entity.encodedSize() <= maxPayloadSize) return listOf(entity)

        return when (entity) {
            is SyncEntityScale.Devices -> splitItems(entity.devices).map(SyncEntityScale::Devices)
            is SyncEntityScale.ChatsAdded -> splitItems(entity.chats).map(SyncEntityScale::ChatsAdded)
            is SyncEntityScale.ChatsRemoved -> splitItems(entity.chats).map(SyncEntityScale::ChatsRemoved)
            is SyncEntityScale.Messages -> splitItems(entity.messages).map(SyncEntityScale::Messages)
        }
    }

    private inline fun <reified T> splitItems(items: List<T>): List<List<T>> {
        return greedyPack(items) { BinaryScale.encodeToByteArray<T>(it).size.bytes }
    }

    // An item larger than the budget still gets a batch of its own.
    private inline fun <T> greedyPack(items: List<T>, sizeOf: (T) -> InformationSize): List<List<T>> {
        val batches = mutableListOf<List<T>>()
        var current = mutableListOf<T>()
        var currentSize = InformationSize.ZERO

        for (item in items) {
            val size = sizeOf(item)
            if (current.isNotEmpty() && currentSize + size > maxPayloadSize) {
                batches.add(current)
                current = mutableListOf()
                currentSize = InformationSize.ZERO
            }
            current.add(item)
            currentSize += size
        }

        if (current.isNotEmpty()) batches.add(current)

        return batches
    }

    private fun SyncEntityScale.encodedSize(): InformationSize =
        BinaryScale.encodeToByteArray<SyncEntityScale>(this).size.bytes
}
