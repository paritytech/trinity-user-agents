package io.paritytech.polkadotapp.chains.network.rpc

import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.novasama.substrate_sdk_android.scale.dataType.DataType
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.mappers.nonNull
import io.novasama.substrate_sdk_android.wsrpc.mappers.pojo
import io.novasama.substrate_sdk_android.wsrpc.request.DeliveryType
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.author.SubmitAndWatchExtrinsicRequest
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.author.SubmitExtrinsicRequest
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.chain.RuntimeVersion
import io.novasama.substrate_sdk_android.wsrpc.subscription.response.SubscriptionChange
import io.novasama.substrate_sdk_android.wsrpc.subscriptionFlow
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.extrinsic.asExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.getSocket
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.model.FeeResponse
import io.paritytech.polkadotapp.chains.network.rpc.model.SignedBlock
import io.paritytech.polkadotapp.chains.network.rpc.requests.GetBlockHashRequest
import io.paritytech.polkadotapp.chains.network.rpc.requests.GetBlockRequest
import io.paritytech.polkadotapp.chains.network.rpc.requests.GetFinalizedHeadRequest
import io.paritytech.polkadotapp.chains.network.rpc.requests.GetHeaderRequest
import io.paritytech.polkadotapp.chains.network.rpc.requests.GetStorageSize
import io.paritytech.polkadotapp.chains.network.rpc.requests.NextAccountIndexRequest
import io.paritytech.polkadotapp.chains.network.rpc.requests.SubscribeFinalizedHeads
import io.paritytech.polkadotapp.chains.network.rpc.requests.SubscribeNewHeads
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.extrinsicHash
import io.paritytech.polkadotapp.chains.util.fromHex
import io.paritytech.polkadotapp.chains.util.hexBytesSize
import io.paritytech.polkadotapp.common.utils.asGsonParsedNumber
import io.paritytech.polkadotapp.common.utils.flowOfAll
import io.paritytech.polkadotapp.common.utils.orZero
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import java.math.BigInteger

data class RpcCalls(
    private val chainRegistry: ChainRegistry,
    private val runtimeCallsApi: MultiChainRuntimeCallsApi,
) {
    suspend fun getExtrinsicFee(chainId: ChainId, extrinsic: SendableExtrinsic): FeeResponse {
        return runtimeCallsApi.forChain(chainId).call(
            section = "TransactionPaymentApi",
            method = "query_info",
            arguments = autoEncodedArgs(
                // rpc needs bytes without length as it adds length in bytes during "uxt" encoding
                "uxt" to extrinsic.bytesWithoutLength,
                "len" to extrinsic.extrinsicHex.hexBytesSize().toBigInteger()
            ),
        )
    }

    suspend fun submitExtrinsic(
        chainId: ChainId,
        extrinsic: String,
    ): String {
        return socketFor(chainId).submitExtrinsic(extrinsic)
    }

    fun submitAndWatchExtrinsic(
        chainId: ChainId,
        extrinsic: String,
    ): Flow<ExtrinsicStatus> {
        return flowOfAll {
            val socket = socketFor(chainId)
            socket.submitAndWatchExtrinsic(extrinsic)
        }
    }

    suspend fun getNonce(
        chainId: ChainId,
        accountAddress: String,
    ): BigInteger {
        val nonceRequest = NextAccountIndexRequest(accountAddress)

        val response = socketFor(chainId).executeAsync(nonceRequest)
        val doubleResult = response.result as Double

        return doubleResult.toInt().toBigInteger()
    }

    suspend fun getRuntimeVersion(chainId: ChainId): RuntimeVersion {
        val request = StateRuntimeVersionRequest()

        return socketFor(chainId).executeAsync(request, mapper = pojo<RuntimeVersion>().nonNull())
    }

    /**
     * Retrieves the block with given hash
     * If hash is null, than the latest block is returned
     */
    suspend fun getBlock(
        chainId: ChainId,
        hash: String? = null,
    ): SignedBlock {
        val blockRequest = GetBlockRequest(hash)

        return socketFor(chainId).executeAsync(blockRequest, mapper = pojo<SignedBlock>().nonNull())
    }

    /**
     * Get hash of the last finalized block in the canon chain
     */
    suspend fun getFinalizedHead(chainId: ChainId): String {
        return socketFor(chainId).executeAsync(GetFinalizedHeadRequest, mapper = pojo<String>().nonNull())
    }

    /**
     * Retrieves the header for a specific block
     *
     * @param hash - hash of the block. If null - then the  best pending header is returned
     */
    suspend fun getBlockHeader(
        chainId: ChainId,
        hash: String? = null,
    ): SignedBlock.Block.Header {
        return socketFor(chainId).executeAsync(GetHeaderRequest(hash), mapper = pojo<SignedBlock.Block.Header>().nonNull())
    }

    suspend fun subscribeNewHeads(
        chainId: ChainId,
    ): Flow<SignedBlock.Block.Header> {
        val socket = socketFor(chainId)
        return socket.subscriptionFlow(SubscribeNewHeads())
            .map { socket.parseSubscriptionChange(it) }
    }

    suspend fun subscribeFinalizedHeads(
        chainId: ChainId,
    ): Flow<SignedBlock.Block.Header> {
        val socket = socketFor(chainId)
        return socket.subscriptionFlow(SubscribeFinalizedHeads())
            .map { socket.parseSubscriptionChange(it) }
    }

    /**
     * Retrieves the hash of a specific block
     *
     *  @param blockNumber - if null, then the  best block hash is returned
     */
    suspend fun getBlockHash(
        chainId: ChainId,
        blockNumber: BlockNumber? = null,
    ): String {
        return socketFor(chainId).executeAsync(GetBlockHashRequest(blockNumber?.value), mapper = pojo<String>().nonNull())
    }

    suspend fun getStorageSize(
        chainId: ChainId,
        storageKey: String,
    ): BigInteger {
        return socketFor(chainId).executeAsync(GetStorageSize(storageKey)).result?.asGsonParsedNumber().orZero()
    }

    private suspend fun socketFor(chainId: ChainId) = chainRegistry.getSocket(chainId)

    private inline fun <reified T> SocketService.parseSubscriptionChange(change: SubscriptionChange): T {
        val tree = jsonMapper.toJsonTree(change.params.result)
        return jsonMapper.fromJson(tree, T::class.java)
    }
}

suspend fun SocketService.stateCall(request: RuntimeRequest): String? {
    return executeAsync(request, mapper = pojo<String>()).result
}

suspend fun <T> SocketService.stateCall(request: RuntimeRequest, returnType: DataType<T>): T {
    val rawResult = stateCall(request)
    requireNotNull(rawResult) {
        "Unexpected state call null response"
    }

    return returnType.fromHex(rawResult)
}

suspend fun SocketService.submitExtrinsic(extrinsic: String): String {
    val request = SubmitExtrinsicRequest(extrinsic)

    return executeAsync(
        request = request,
        mapper = pojo<String>().nonNull(),
        deliveryType = DeliveryType.AT_MOST_ONCE
    )
}

fun SocketService.submitAndWatchExtrinsic(extrinsic: String): Flow<ExtrinsicStatus> {
    val hash = extrinsic.extrinsicHash()
    val request = SubmitAndWatchExtrinsicRequest(extrinsic)

    var atLeastOneUpdateReceivedFromNode = false

    return subscriptionFlow(request, unsubscribeMethod = "author_unwatchExtrinsic")
        .onEach { atLeastOneUpdateReceivedFromNode = true }
        .map { it.asExtrinsicStatus(hash) }
        .catch {
            if (atLeastOneUpdateReceivedFromNode) {
                emit(ExtrinsicStatus.Other(rawStatus = it.toString(), extrinsicHash = hash))
            } else {
                emit(ExtrinsicStatus.FailedToSubmit(it))
            }
        }
}

suspend fun RpcCalls.getBlockNumber(chainId: ChainId, blockHash: String): Int {
    return getBlockHeader(chainId, blockHash).number
}
