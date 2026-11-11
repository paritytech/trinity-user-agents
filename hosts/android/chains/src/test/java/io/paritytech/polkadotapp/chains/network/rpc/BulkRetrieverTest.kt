package io.paritytech.polkadotapp.chains.network.rpc

import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.mappers.ResponseMapper
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

/**
 * Prefix queries page through `state_getKeysPaged` whether or not they are pinned to a block.
 *
 * Nodes running with safe RPC methods only refuse `state_getKeys`, so a pinned query cannot fall back to it.
 * The paged call takes its params positionally, which means a pinned first page has to send an explicit
 * `null` start key for the block hash to land in the fourth slot: a request that merely omitted the start
 * key would hand the block hash over as the start key.
 */
class BulkRetrieverTest {
    @Before
    fun mockExecuteAsync() = mockkStatic(COROUTINE_ADAPTER_FILE)

    @After
    fun unmockExecuteAsync() = unmockkStatic(COROUTINE_ADAPTER_FILE)

    private val socketService: SocketService = mockk()
    private val gson = Gson()
    private val sent = mutableListOf<RuntimeRequest>()
    private val retriever = BulkRetriever(pageSize = 2)

    @Test
    fun `keys pinned to a block are paged with the block hash in the fourth slot`() = runTest {
        answerPages(listOf("0x01", "0x02"), listOf("0x03"))

        val keys = retriever.retrieveAllKeys(socketService, PREFIX, at = BLOCK)

        assertEquals(listOf("0x01", "0x02", "0x03"), keys)
        assertEquals(
            listOf(
                "state_getKeysPaged" to """["$PREFIX",2,null,"$BLOCK"]""",
                "state_getKeysPaged" to """["$PREFIX",2,"0x02","$BLOCK"]""",
            ),
            sentRequests()
        )
    }

    @Test
    fun `keys at the best block are paged without a block hash`() = runTest {
        answerPages(listOf("0x01", "0x02"), emptyList())

        val keys = retriever.retrieveAllKeys(socketService, PREFIX)

        assertEquals(listOf("0x01", "0x02"), keys)
        assertEquals(
            listOf(
                "state_getKeysPaged" to """["$PREFIX",2]""",
                "state_getKeysPaged" to """["$PREFIX",2,"0x02"]""",
            ),
            sentRequests()
        )
    }

    private fun answerPages(vararg pages: List<String>) {
        val remaining = pages.toMutableList()

        coEvery { socketService.executeAsync(any(), any(), any<ResponseMapper<List<String>>>()) } coAnswers {
            sent += secondArg<RuntimeRequest>()
            remaining.removeAt(0)
        }
    }

    /** Each request as the node sees it: the method and the JSON of its positional params. */
    private fun sentRequests() = sent.map { it.method to gson.toJson(it.params) }

    private companion object {
        const val COROUTINE_ADAPTER_FILE = "io.novasama.substrate_sdk_android.wsrpc.CoroutineAdapterKt"
        const val PREFIX = "0xaa"
        const val BLOCK = "0xbb"
    }
}
