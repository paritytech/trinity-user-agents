package io.paritytech.polkadotapp.chains.network.rpc

import com.google.gson.JsonNull
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.mappers.nonNull
import io.novasama.substrate_sdk_android.wsrpc.mappers.pojoList
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.withContext
import kotlin.coroutines.coroutineContext

class GetKeysPagedRequest(
    keyPrefix: String,
    pageSize: Int,
    fullKeyOffset: String?,
    at: BlockHash?,
) : RuntimeRequest(
    method = "state_getKeysPaged",
    params = pagedKeysParams(keyPrefix, pageSize, fullKeyOffset, at)
)

/**
 * Positional params of `state_getKeysPaged`: prefix, page size, start key, block hash.
 *
 * A query pinned to [at] that has no start key yet sends JSON null in the third slot, so the block hash
 * is not read as the start key.
 */
private fun pagedKeysParams(
    keyPrefix: String,
    pageSize: Int,
    fullKeyOffset: String?,
    at: BlockHash?,
): List<Any> {
    val offset = fullKeyOffset ?: JsonNull.INSTANCE.takeIf { at != null }

    return listOfNotNull(keyPrefix, pageSize, offset, at)
}

class QueryStorageAtRequest(
    keys: List<String>,
    at: String?,
) : RuntimeRequest(
    method = "state_queryStorageAt",
    params =
    listOfNotNull(
        keys,
        at
    )
)

class QueryStorageAtResponse(
    val block: String,
    val changes: List<List<String?>>,
) {
    fun changesAsMap(): Map<String, String?> {
        return changes.map { it[0]!! to it[1] }.toMap()
    }
}

class BulkRetriever(private val pageSize: Int) {
    /**
     * Retrieves all keys starting with [keyPrefix] from [at] block, or from the best block when [at] is null
     */
    suspend fun retrieveAllKeys(
        socketService: SocketService,
        keyPrefix: String,
        at: BlockHash? = null,
    ): List<String> =
        withContext(Dispatchers.IO) {
            queryKeysByPrefix(socketService, keyPrefix, at)
        }

    suspend fun queryKeys(
        socketService: SocketService,
        keys: List<String>,
        at: BlockHash? = null,
    ): Map<String, String?> =
        withContext(Dispatchers.IO) {
            val chunks = keys.chunked(pageSize)

            chunks.fold(mutableMapOf()) { acc, chunk ->
                ensureActive()

                val request = QueryStorageAtRequest(chunk, at)

                val chunkValues =
                    socketService.executeAsync(request, mapper = pojoList<QueryStorageAtResponse>().nonNull())
                        .first().changesAsMap()

                acc.putAll(chunkValues)

                acc
            }
        }

    private suspend fun queryKeysByPrefix(
        socketService: SocketService,
        prefix: String,
        at: BlockHash?,
    ): List<String> {
        val result = mutableListOf<String>()

        var currentOffset: String? = null

        while (true) {
            coroutineContext.ensureActive()

            val request = GetKeysPagedRequest(prefix, pageSize, currentOffset, at)

            val page = socketService.executeAsync(request, mapper = pojoList<String>().nonNull())

            result += page

            if (isLastPage(page)) break

            currentOffset = page.last()
        }

        return result
    }

    private fun isLastPage(page: List<String>) = page.size < pageSize
}

suspend fun BulkRetriever.retrieveAllValues(
    socketService: SocketService,
    keyPrefix: String,
    at: BlockHash? = null,
): Map<String, String?> {
    val allKeys = retrieveAllKeys(socketService, keyPrefix, at)

    return queryKeys(socketService, allKeys, at)
}

suspend fun BulkRetriever.queryKey(
    socketService: SocketService,
    key: String,
    at: BlockHash? = null,
): String? = queryKeys(socketService, listOf(key), at).values.first()
