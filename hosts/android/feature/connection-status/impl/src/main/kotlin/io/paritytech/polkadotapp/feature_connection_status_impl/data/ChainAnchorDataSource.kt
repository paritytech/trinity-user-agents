package io.paritytech.polkadotapp.feature_connection_status_impl.data

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.chains.storage.source.query.api.queryNonNull
import io.paritytech.polkadotapp.chains.storage.source.query.metadata
import io.paritytech.polkadotapp.chains.storage.source.queryCatching
import io.paritytech.polkadotapp.chains.storage.typed.now
import io.paritytech.polkadotapp.chains.storage.typed.timestamp
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchor
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchorSource
import java.math.BigInteger
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

class ChainAnchorDataSource @Inject constructor(
    private val rpcCalls: RpcCalls,
    @param:RemoteSourceQualifier private val remoteStorage: StorageDataSource,
) : BlockProductionAnchorSource {
    override suspend fun fetch(chainId: ChainId, expectedBlocks: Int): Result<BlockProductionAnchor> =
        runCancellableCatching { readHead(chainId) }
            .flatMap { head ->
                if (head.height < expectedBlocks) {
                    Result.failure(IllegalStateException("$chainId is ${head.height} blocks long, fewer than the $expectedBlocks asked for"))
                } else {
                    spanBack(chainId, head, expectedBlocks)
                }
            }
            .flatMap { span ->
                if (span.isPositive()) {
                    Result.success(BlockProductionAnchor(headHeight = span.head.height, blocks = expectedBlocks, chainClockSpan = span.duration))
                } else {
                    Result.failure(IllegalStateException("$chainId timestamps do not move forward across the last $expectedBlocks blocks: ${span.duration}"))
                }
            }

    // Height read off the head hash, so the two cannot land a block apart and shrink the span by that block.
    private suspend fun readHead(chainId: ChainId): Head {
        val hash = rpcCalls.getBlockHash(chainId)
        return Head(hash = hash, height = rpcCalls.getBlockHeader(chainId, hash).number)
    }

    private suspend fun spanBack(chainId: ChainId, head: Head, blocks: Int): Result<Span> =
        runCancellableCatching { rpcCalls.getBlockHash(chainId, (head.height - blocks).toBlockNumber()) }
            .flatMap { previousHash ->
                timestampAt(chainId, head.hash).flatMap { headTime ->
                    timestampAt(chainId, previousHash).map { previousTime ->
                        Span(head = head, duration = (headTime - previousTime).toLong().milliseconds)
                    }
                }
            }

    private suspend fun timestampAt(chainId: ChainId, hash: String): Result<BigInteger> =
        remoteStorage.queryCatching(chainId, at = hash) { metadata.timestamp.now.queryNonNull() }

    private data class Head(val hash: String, val height: Int)

    private data class Span(val head: Head, val duration: Duration) {
        fun isPositive(): Boolean = duration.isPositive()
    }
}
