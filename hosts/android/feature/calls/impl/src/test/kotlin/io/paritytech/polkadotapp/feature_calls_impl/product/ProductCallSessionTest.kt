package io.paritytech.polkadotapp.feature_calls_impl.product

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallSignals
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductMediaCall
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignal
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.RealPeerSignalCoder
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.SdpCoder
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.SdpCoderSetup
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.webrtc.IceCandidate

class ProductCallSessionTest {
    private val coder = RealPeerSignalCoder()
    private val sdpCoder = SdpCoder()
    private val chatId = ChatId.forExtensionRoom("chat.dot", "contact:bob")
    private val call = ProductMediaCall(chatId, callId = "c1", roomName = "Bob", video = true, offerer = true)
    private val signals = mockk<ProductCallSignals>(relaxed = true)
    private val closedSessions = mutableListOf<ProductCallSession>()
    private val session = ProductCallSession(call, signals, coder, onClosed = { closedSessions += it })

    private fun setup(setup: String) = sdpCoder.encodeSetup(
        SdpCoderSetup(
            setupSdp = "o=- 1 2 IN IP4 0.0.0.0\na=ice-ufrag:u\na=ice-pwd:p\na=fingerprint:sha-256 0A:0B\na=setup:$setup\n",
            candidates = listOf(IceCandidate("0", 0, "candidate:1 1 udp 1 10.0.0.1 9 typ host")),
        )
    )

    private fun candidates(address: String) =
        sdpCoder.encodeCandidates(listOf(IceCandidate("0", 0, "candidate:2 1 udp 1 $address 9 typ host")))

    @Test
    fun `the offer goes to the product, and the call rings once a device of the contact read the offer`() = runTest {
        val offer = setup("actpass")

        session.sendOffer(call.callId, chatId, offer, withVideo = true)

        coVerify { signals.send(match { (coder.decode(it).single() as PeerSignal.Offer).setup.contentEquals(offer) }) }
        // As the native call, which says "Ringing" only once the offer message is read (IS_READ).
        assertFalse(session.observeOfferReadStatus(call.callId).first())

        session.markOfferRead()

        assertTrue(session.observeOfferReadStatus(call.callId).first())
    }

    @Test
    fun `the answer and the candidates of the other side reach the engine, the early ones too`() = runTest {
        val answer = setup("active")
        session.receive(coder.encode(PeerSignal.Candidates(candidates("10.0.0.2"))))
        session.receive(coder.encode(PeerSignal.Answer(answer)))
        session.receive(coder.encode(PeerSignal.Candidates(candidates("10.0.0.3"))))

        assertArrayEquals(answer, session.awaitIncomingAnswer(call.callId, chatId))
        val received = session.subscribeIncomingIceCandidates(chatId, call.callId).take(2).toList()
        assertEquals(
            listOf("10.0.0.2", "10.0.0.3"),
            received.map { sdpCoder.decodeCandidates(it).single().sdp.split(" ")[4] },
        )
    }

    @Test
    fun `ending on the call screen tells the product once`() = runTest {
        session.sendCloseSignal(call.callId, chatId)
        session.sendCloseSignal(call.callId, chatId)

        coVerify(exactly = 1) { signals.send(match { coder.decode(it) == listOf(PeerSignal.Closed) }) }
        coVerify(exactly = 1) { signals.ended() }
        assertEquals(listOf(session), closedSessions)
    }

    @Test
    fun `a close of the other side ends the call and sends nothing back`() = runTest {
        session.receive(coder.encode(PeerSignal.Closed))

        session.subscribeIncomingCloseSignal(call.callId, chatId).first()
        session.sendCloseSignal(call.callId, chatId)
        coVerify(exactly = 0) { signals.send(any()) }
        coVerify(exactly = 0) { signals.ended() }
        assertEquals(listOf(session), closedSessions)
    }
}
