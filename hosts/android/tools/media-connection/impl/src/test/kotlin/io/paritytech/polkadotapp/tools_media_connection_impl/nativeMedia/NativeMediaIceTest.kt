package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import io.paritytech.polkadotapp.tools_media_connection_impl.models.ExternalRtcConfig
import io.paritytech.polkadotapp.tools_media_connection_impl.models.TurnCredentials
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.cancelAndJoin
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

@OptIn(ExperimentalCoroutinesApi::class)
class NativeMediaIceTest {
    @Test
    fun `credential lookup cancels a suspended provider after two seconds`() = runTest {
        var cancelled = false
        val operation = async {
            nativeMediaIceServers {
                try {
                    awaitCancellation()
                } finally {
                    cancelled = true
                }
            }
        }
        runCurrent()
        advanceTimeBy(1_999)
        assertFalse(operation.isCompleted)
        advanceTimeBy(1)
        runCurrent()
        assertTrue(operation.await().isEmpty())
        assertTrue(cancelled)
        assertEquals(2_000L, currentTime)
    }

    @Test
    fun `outer cancellation stops credential lookup without direct fallback`() = runTest {
        var cancelled = false
        var returned = false
        val operation = async {
            nativeMediaIceServers {
                try {
                    awaitCancellation()
                } finally {
                    cancelled = true
                }
            }
            returned = true
        }
        runCurrent()
        operation.cancelAndJoin()
        assertTrue(operation.isCancelled)
        assertTrue(cancelled)
        assertFalse(returned)
        assertEquals(0L, currentTime)
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

    private fun servers(): List<PeerConnection.IceServer> =
        listOf(PeerConnection.IceServer.builder("turn:relay.example:3478")
            .setUsername("test")
            .setPassword("test")
            .createIceServer())
}
