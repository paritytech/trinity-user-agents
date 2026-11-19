package io.paritytech.polkadotapp.feature_chats_impl.data.hop

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.test_shared.assertFailureOf
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class HopNodeUrlProviderTest {
    private val chainRegistry: ChainRegistry = mock(ChainRegistry::class.java)
    private val bulletIn: Chain = mock(Chain::class.java)

    private val provider = HopNodeUrlProvider(chainRegistry)

    @Test
    fun `a retry never picks the failed node when others are listed`() = runBlocking<Unit> {
        withHopUrls(NODE_A, NODE_B, NODE_C)

        val picks = repeatedRetryPicks(failed = NODE_A)

        assertTrue("picks: $picks", picks.all { it in setOf(NODE_B, NODE_C) })
    }

    @Test
    fun `a retry picks only listed nodes when the failed node was removed`() = runBlocking<Unit> {
        withHopUrls(NODE_B, NODE_C)

        val picks = repeatedRetryPicks(failed = NODE_A)

        assertTrue("picks: $picks", picks.all { it in setOf(NODE_B, NODE_C) })
    }

    @Test
    fun `a retry goes to the first node when it failed on the second of two`() = runBlocking<Unit> {
        withHopUrls(NODE_A, NODE_B)

        val picks = repeatedRetryPicks(failed = NODE_B)

        assertEquals(setOf(NODE_A), picks.toSet())
    }

    @Test
    fun `a retry reuses the failed node when it is the only one listed`() = runBlocking<Unit> {
        withHopUrls(NODE_A)

        assertEquals(NODE_A, provider.pickForRetry(failedNodeUrl = NODE_A))
    }

    @Test
    fun `a retry reuses the chain's first node when the chain lists no Hop API`() = runBlocking<Unit> {
        withNoHopApiAndChainNodes(NODE_A)

        assertEquals(NODE_A, provider.pickForRetry(failedNodeUrl = NODE_A))
    }

    @Test
    fun `retries spread over every other node when three are listed`() = runBlocking<Unit> {
        withHopUrls(NODE_A, NODE_B, NODE_C)

        val picks = repeatedRetryPicks(failed = NODE_A)

        assertEquals(setOf(NODE_B, NODE_C), picks.toSet())
    }

    @Test
    fun `a retry throws when no node is available`() = runBlocking<Unit> {
        withNoHopApiAndChainNodes()

        val result = runCatching { provider.pickForRetry(failedNodeUrl = NODE_A) }

        assertFailureOf<IllegalStateException>(result)
    }

    private suspend fun repeatedRetryPicks(failed: String): List<String> {
        return List(RANDOM_PICK_REPETITIONS) { provider.pickForRetry(failedNodeUrl = failed) }
    }

    private suspend fun withHopUrls(vararg urls: String) {
        whenever(chainRegistry.bulletInChain()).thenReturn(bulletIn)
        whenever(bulletIn.externalApis).thenReturn(listOf(Chain.ExternalApi.Hop(urls.toList())))
    }

    private suspend fun withNoHopApiAndChainNodes(vararg urls: String) {
        val nodes = urls.map { url ->
            mock(Chain.Node::class.java).also { whenever(it.unformattedUrl).thenReturn(url) }
        }
        val chainNodes = mock(Chain.Nodes::class.java)
        whenever(chainNodes.nodes).thenReturn(nodes)

        whenever(chainRegistry.bulletInChain()).thenReturn(bulletIn)
        whenever(bulletIn.externalApis).thenReturn(emptyList())
        whenever(bulletIn.nodes).thenReturn(chainNodes)
    }

    private companion object {
        const val RANDOM_PICK_REPETITIONS = 200
    }
}
