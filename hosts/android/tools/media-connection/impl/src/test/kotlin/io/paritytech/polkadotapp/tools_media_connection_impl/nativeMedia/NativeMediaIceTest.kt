package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.TurnCredentials
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.ExternalRtcConfigProvider
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnApi
import io.paritytech.polkadotapp.tools_media_connection_impl.turn.TurnCredentialsResponse
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.webrtc.PeerConnection
import kotlin.time.Instant

@OptIn(ExperimentalCoroutinesApi::class)
class NativeMediaIceTest {
    @Test
    fun `slow TURN response warms the shared cache after Media returns direct ICE`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                delay(3_000)
                credentials()
            } catch (failure: CancellationException) {
                cancelled = true
                throw failure
            }
        }
        val provider = provider(api)
        val firstPeer = async { nativeMediaIceServers(provider::getConfig) }
        runCurrent()
        advanceTimeBy(1_999)
        assertFalse(firstPeer.isCompleted)
        advanceTimeBy(1)
        runCurrent()
        assertTrue(firstPeer.await().isEmpty())
        assertEquals(2_000L, currentTime)
        assertFalse(cancelled)

        advanceTimeBy(1_000)
        runCurrent()
        val secondPeer = nativeMediaIceServers(provider::getConfig)
        assertTrue(secondPeer.any { "turn:relay.example:3478" in it.urls })
        assertEquals(3_000L, currentTime)
        assertFalse(cancelled)
        coVerify(exactly = 1) { api.issueTurnCredentials(any()) }
    }

    @Test
    fun `a suspended shared request stops at its own fifteen second deadline`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                awaitCancellation()
            } finally {
                cancelled = true
            }
        }
        val provider = provider(api)
        val firstPeer = async { nativeMediaIceServers(provider::getConfig) }
        runCurrent()
        advanceTimeBy(2_000)
        runCurrent()
        assertTrue(firstPeer.await().isEmpty())
        assertFalse(cancelled)
        advanceTimeBy(12_999)
        runCurrent()
        assertFalse(cancelled)
        advanceTimeBy(1)
        runCurrent()
        assertTrue(cancelled)
        coVerify(exactly = 1) { api.issueTurnCredentials(any()) }

        coEvery { api.issueTurnCredentials(any()) } returns credentials()
        assertTrue(nativeMediaIceServers(provider::getConfig).isNotEmpty())
        coVerify(exactly = 2) { api.issueTurnCredentials(any()) }
    }

    @Test
    fun `outer cancellation stops credential lookup without direct fallback`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        var returned = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                awaitCancellation()
            } finally {
                cancelled = true
            }
        }
        val provider = provider(api)
        val operation = async {
            nativeMediaIceServers(provider::getConfig)
            returned = true
        }
        runCurrent()
        operation.cancelAndJoin()
        runCurrent()
        assertTrue(operation.isCancelled)
        assertTrue(cancelled)
        assertFalse(returned)
        assertEquals(0L, currentTime)
    }

    @Test
    fun `cancelling Media does not cancel a concurrent Chat lookup`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                delay(3_000)
                credentials()
            } catch (failure: CancellationException) {
                cancelled = true
                throw failure
            }
        }
        val provider = provider(api)
        val media = async { nativeMediaIceServers(provider::getConfig) }
        val chat = async { provider.getConfig() }
        runCurrent()
        media.cancelAndJoin()
        runCurrent()
        assertFalse(cancelled)
        assertFalse(chat.isCompleted)

        advanceTimeBy(3_000)
        runCurrent()
        assertTrue(chat.await().turnCredentials.isNotEmpty())
        assertTrue(nativeMediaIceServers(provider::getConfig).isNotEmpty())
        assertFalse(cancelled)
        coVerify(exactly = 1) { api.issueTurnCredentials(any()) }
    }

    @Test
    fun `cancelling Chat does not cancel Media or its cache warmup`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                delay(3_000)
                credentials()
            } catch (failure: CancellationException) {
                cancelled = true
                throw failure
            }
        }
        val provider = provider(api)
        val chat = async { provider.getConfig() }
        val media = async { nativeMediaIceServers(provider::getConfig) }
        runCurrent()
        chat.cancelAndJoin()
        advanceTimeBy(2_000)
        runCurrent()
        assertTrue(media.await().isEmpty())
        assertFalse(cancelled)

        advanceTimeBy(1_000)
        runCurrent()
        assertTrue(nativeMediaIceServers(provider::getConfig).isNotEmpty())
        assertFalse(cancelled)
        coVerify(exactly = 1) { api.issueTurnCredentials(any()) }
    }

    @Test
    fun `cancelling the last consumer stops the shared request`() = runTest {
        val api = mockk<TurnApi>()
        var cancelled = false
        coEvery { api.issueTurnCredentials(any()) } coAnswers {
            try {
                awaitCancellation()
            } finally {
                cancelled = true
            }
        }
        val provider = provider(api)
        val media = async { nativeMediaIceServers(provider::getConfig) }
        val chat = async { provider.getConfig() }
        runCurrent()
        media.cancelAndJoin()
        runCurrent()
        assertFalse(cancelled)
        chat.cancelAndJoin()
        runCurrent()
        assertTrue(cancelled)
        assertEquals(0L, currentTime)
        coVerify(exactly = 1) { api.issueTurnCredentials(any()) }

        coEvery { api.issueTurnCredentials(any()) } returns credentials()
        assertTrue(provider.getConfig().turnCredentials.isNotEmpty())
        coVerify(exactly = 2) { api.issueTurnCredentials(any()) }
    }

    @Test
    fun `provider-local cancellation and errors leave direct ICE available`() = runTest {
        assertTrue(nativeMediaIceServers { throw CancellationException("provider stopped") }.isEmpty())
        assertTrue(nativeMediaIceServers { throw IllegalStateException("provider unavailable") }.isEmpty())
    }

    @Test
    fun `available credentials supplement direct ICE`() = runTest {
        val servers = nativeMediaIceServers {
            ExternalRtcConfig(listOf(TurnCredentials("turn:relay.example:3478", "test", "test")))
        }
        assertTrue(servers.any { "turn:relay.example:3478" in it.urls })
        assertEquals(0L, currentTime)
    }

    @Test
    fun `rejected optional configuration retries an empty direct configuration`() {
        for (throws in listOf(false, true)) {
            var attempts = 0
            val connection = Any()
            val result = createNativeMediaPeerConnection(servers()) { configuration ->
                attempts += 1
                assertEquals(PeerConnection.IceTransportsType.ALL, configuration.iceTransportsType)
                assertEquals(PeerConnection.SdpSemantics.UNIFIED_PLAN, configuration.sdpSemantics)
                assertEquals(PeerConnection.BundlePolicy.MAXBUNDLE, configuration.bundlePolicy)
                assertEquals(PeerConnection.RtcpMuxPolicy.REQUIRE, configuration.rtcpMuxPolicy)
                if (attempts == 1) {
                    assertFalse(configuration.iceServers.isEmpty())
                    if (throws) throw IllegalArgumentException("optional server rejected")
                    null
                } else {
                    assertTrue(configuration.iceServers.isEmpty())
                    connection
                }
            }
            assertSame(connection, result)
            assertEquals(2, attempts)
        }
    }

    @Test
    fun `accepted optional configuration is not retried`() {
        var attempts = 0
        val connection = Any()
        assertSame(connection, createNativeMediaPeerConnection(servers()) {
            attempts += 1
            connection
        })
        assertEquals(1, attempts)
    }

    @Test
    fun `direct factory failure is preserved without another retry`() {
        for (configured in listOf(false, true)) {
            var attempts = 0
            val failure = IllegalStateException("direct creation failed")
            val thrown = assertThrows(IllegalStateException::class.java) {
                createNativeMediaPeerConnection<Any>(if (configured) servers() else emptyList()) {
                    attempts += 1
                    if (configured && attempts == 1) null else throw failure
                }
            }
            assertSame(failure, thrown)
            assertEquals(if (configured) 2 else 1, attempts)
        }
    }

    @Test
    fun `direct nil remains a creation failure`() {
        var attempts = 0
        assertThrows(IllegalArgumentException::class.java) {
            createNativeMediaPeerConnection<Any>(emptyList()) {
                attempts += 1
                null
            }
        }
        assertEquals(1, attempts)
    }

    @Test
    fun `factory cancellation is never retried`() {
        var attempts = 0
        val failure = CancellationException("creation cancelled")
        val thrown = assertThrows(CancellationException::class.java) {
            createNativeMediaPeerConnection<Any>(servers()) {
                attempts += 1
                throw failure
            }
        }
        assertSame(failure, thrown)
        assertEquals(1, attempts)
    }

    private fun TestScope.provider(api: TurnApi) = ExternalRtcConfigProvider(
        api,
        CurrentTimeContext { Instant.fromEpochMilliseconds(currentTime) }
    )

    private fun credentials() = TurnCredentialsResponse(
        servers = listOf("turn:relay.example:3478"),
        username = "test",
        password = "test",
        ttl = 60
    )

    private fun servers(): List<PeerConnection.IceServer> =
        listOf(PeerConnection.IceServer.builder("turn:relay.example:3478")
            .setUsername("test")
            .setPassword("test")
            .createIceServer())
}
