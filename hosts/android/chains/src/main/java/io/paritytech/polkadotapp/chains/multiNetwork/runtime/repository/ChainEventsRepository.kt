package io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository

import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHexOrNull
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Extrinsic
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericEvent
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.chains.network.binding.EventRecord
import io.paritytech.polkadotapp.chains.network.binding.Phase
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.storage.source.query.RemoteStorageQueryContextFactory
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.typed.events
import io.paritytech.polkadotapp.chains.storage.typed.system
import io.paritytech.polkadotapp.chains.util.extrinsicHash

interface ChainEventsRepository {
    suspend fun getEventsInBlock(blockHash: BlockHash? = null): BlockEvents
}

suspend fun ChainEventsRepository.getExtrinsicWithEvents(
    extrinsicHash: String,
    blockHash: BlockHash? = null
): ExtrinsicWithEvents? {
    val allExtrinsics = getEventsInBlock(blockHash).applyExtrinsic
    return allExtrinsics.find { it.extrinsicHash == extrinsicHash }
}

class ChainEventsRepositoryFactory(
    private val remoteStorageQueryContextFactory: RemoteStorageQueryContextFactory,
    private val rpcCalls: RpcCalls,
) {
    fun create(chainId: ChainId): ChainEventsRepository {
        return RealChainEventsRepository(
            rpcCalls = rpcCalls,
            remoteStorageQueryContextFactory = remoteStorageQueryContextFactory,
            chainId = chainId
        )
    }
}

internal class RealChainEventsRepository(
    private val rpcCalls: RpcCalls,
    private val remoteStorageQueryContextFactory: RemoteStorageQueryContextFactory,
    private val chainId: ChainId,
) : ChainEventsRepository {
    override suspend fun getEventsInBlock(
        blockHash: BlockHash?,
    ): BlockEvents {
        return query(chainId, at = blockHash) {
            val eventRecords = metadata.system.events.query().orEmpty()
            val block = rpcCalls.getBlock(chainId, blockHash)

            BlockEvents(
                initialization = eventRecords.mapNotNull { record -> record.event.takeIf { record.phase is Phase.Initialization } },
                applyExtrinsic = groupExtrinsicWithEvents(eventRecords, block.block.extrinsics),
                finalization = eventRecords.mapNotNull { record -> record.event.takeIf { record.phase is Phase.Finalization } }
            )
        }
    }

    context(storage: StorageQueryContext)
    private fun groupExtrinsicWithEvents(
        eventRecords: List<EventRecord>,
        extrinsics: List<String>
    ): List<ExtrinsicWithEvents> {
        val eventsByExtrinsicIndex: Map<Int, List<GenericEvent.Instance>> = eventRecords.mapNotNull { eventRecord ->
            (eventRecord.phase as? Phase.ApplyExtrinsic)?.let {
                it.extrinsicId.toInt() to eventRecord.event
            }
        }.groupBy(
            keySelector = { it.first },
            valueTransform = { it.second }
        )

        return extrinsics.mapIndexedNotNull { index, extrinsicScale ->
            val decodedExtrinsic = Extrinsic.fromHexOrNull(storage.runtime, extrinsicScale)

            decodedExtrinsic?.let {
                val extrinsicEvents = eventsByExtrinsicIndex[index] ?: emptyList()

                ExtrinsicWithEvents(
                    extrinsic = decodedExtrinsic,
                    extrinsicHash = extrinsicScale.extrinsicHash(),
                    events = extrinsicEvents
                )
            }
        }
    }

    private suspend fun <R> query(
        chainId: String,
        at: BlockHash? = null,
        query: suspend StorageQueryContext.() -> R,
    ): R {
        val context = remoteStorageQueryContextFactory.create(chainId, at = at)
        return context.query()
    }
}
