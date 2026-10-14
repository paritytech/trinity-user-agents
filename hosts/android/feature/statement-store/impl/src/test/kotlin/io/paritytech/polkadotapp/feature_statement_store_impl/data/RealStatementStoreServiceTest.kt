package io.paritytech.polkadotapp.feature_statement_store_impl.data

import com.google.gson.Gson
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.novasama.substrate_sdk_android.wsrpc.subscription.response.SubscriptionChange
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnection
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

// Nothing else in the suite fails if subscribeStatements stops running through the peer.
class RealStatementStoreServiceTest {
    private val chainRegistry: ChainRegistry = mock(ChainRegistry::class.java)
    private val connection: ChainConnection = mock(ChainConnection::class.java)
    private val socketService: SocketService = mock(SocketService::class.java)
    private val peer = RealStatementStorePeer(
        KnownChains(people = PEOPLE, assetHub = "hub", bulletIn = "bulletin", hydration = null),
        handoverGrace = GRACE,
    )
    private val service = RealStatementStoreService(chainRegistry, peer, Gson())

    @Before
    fun setUp() = runBlocking<Unit> {
        whenever(chainRegistry.getConnection(PEOPLE)).thenReturn(connection)
        whenever(connection.socketService).thenReturn(socketService)
    }

    @Test
    fun `a page from a statement subscription answers the peer`() = runBlocking<Unit> {
        withPeerSending(emptyPage())

        collecting {
            val reported = withTimeoutOrNull(TIMEOUT) { peer.observeAnswered().first { it } } != null

            assertTrue("subscribeStatements did not report its page through the peer", reported)
        }
    }

    private fun withPeerSending(change: SubscriptionChange) {
        whenever(socketService.subscribe(any<RuntimeRequest>(), any(), any())).thenAnswer { invocation ->
            val listener = invocation.getArgument<SocketService.ResponseListener<SubscriptionChange>>(1)
            listener.onNext(change)
            mock(SocketService.Cancellable::class.java)
        }
    }

    private suspend fun collecting(assertions: suspend () -> Unit) = coroutineScope {
        val collection: Job = launch { service.subscribeStatements(FILTER).collect { } }

        try {
            assertions()
        } finally {
            collection.cancelAndJoin()
        }
    }

    private fun emptyPage() = SubscriptionChange(
        jsonrpc = "2.0",
        method = "statement_subscribeStatement",
        params = SubscriptionChange.Params(
            result = mapOf("event" to "statements", "data" to mapOf("statements" to emptyList<String>(), "remaining" to 0)),
            subscription = "sub-1",
        ),
    )

    private companion object {
        const val PEOPLE = "people"
        val FILTER = TopicFilter.MatchAll(listOf("topic".toByteArray()))
        val GRACE = 300.milliseconds
        val TIMEOUT = 5.seconds
    }
}
