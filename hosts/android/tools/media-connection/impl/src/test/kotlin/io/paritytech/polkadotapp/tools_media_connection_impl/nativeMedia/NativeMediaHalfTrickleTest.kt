package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.tools_media_connection_impl.WebRtcCore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStreamTrack
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import uniffi.truapi.MediaBackendEvent
import uniffi.truapi.MediaDescription
import uniffi.truapi.MediaDescriptionKind

@OptIn(ExperimentalCoroutinesApi::class)
class NativeMediaHalfTrickleTest {
    @Test
    fun `completion publishes current offer once and only later ICE trickles`() = runTest {
        val fixture = PeerFixture(this)
        try {
            val start = async { fixture.peer.start() }
            runCurrent()
            fixture.observer.onIceCandidate(candidate())
            runCurrent()
            assertTrue(fixture.signals().isEmpty())
            fixture.complete()
            start.await()
            assertEquals(listOf(MediaDescriptionKind.OFFER), fixture.descriptions().map { it.description.kind })
            assertEquals(sdp(), fixture.descriptions().single().description.sdp)
            assertTrue(fixture.ice().isEmpty())

            fixture.observer.onIceCandidate(candidate()) // Delayed callback already in the SDP.
            fixture.observer.onIceCandidate(candidate(port = 5001))
            fixture.observer.onIceCandidate(candidate(port = 5001))
            runCurrent()
            assertEquals(1, fixture.ice().size)
            assertTrue(fixture.ice().single().candidate.candidate.contains(" 5001 typ host"))
        } finally { fixture.peer.close() }
    }

    @Test
    fun `one second deadline publishes partial SDP without waiting for TURN`() = runTest {
        val fixture = PeerFixture(this)
        try {
            val start = async { fixture.peer.start() }
            runCurrent()
            fixture.current = SessionDescription(SessionDescription.Type.OFFER, sdp())
            fixture.observer.onIceCandidate(candidate())
            advanceTimeBy(NATIVE_MEDIA_ICE_GATHERING_DEADLINE_MS - 1)
            runCurrent()
            assertTrue(fixture.signals().isEmpty())
            advanceTimeBy(1)
            runCurrent()
            start.await()
            assertEquals(sdp(), fixture.descriptions().single().description.sdp)
            assertTrue(fixture.ice().isEmpty())
            fixture.observer.onIceCandidate(candidate(port = 5001))
            fixture.complete()
            runCurrent()
            assertEquals(1, fixture.descriptions().size)
            assertEquals(1, fixture.ice().size)
        } finally { fixture.peer.close() }
    }

    @Test
    fun `answers gather into their current local SDP too`() = runTest {
        val fixture = PeerFixture(this, offerer = false)
        try {
            fixture.peer.start()
            val answer = async { fixture.peer.description(MediaDescription(MediaDescriptionKind.OFFER, sdp())) }
            runCurrent()
            fixture.observer.onIceCandidate(candidate())
            runCurrent()
            assertTrue(fixture.signals().isEmpty())
            fixture.complete(type = SessionDescription.Type.ANSWER)
            answer.await()
            assertEquals(MediaDescriptionKind.ANSWER, fixture.descriptions().single().description.kind)
            assertEquals(sdp(), fixture.descriptions().single().description.sdp)
            assertTrue(fixture.ice().isEmpty())
        } finally { fixture.peer.close() }
    }

    @Test
    fun `new ICE credentials start a fresh gathering gate`() = runTest {
        val fixture = PeerFixture(this)
        try {
            val start = async { fixture.peer.start() }
            runCurrent()
            fixture.complete()
            start.await()
            fixture.gathering = PeerConnection.IceGatheringState.GATHERING
            fixture.peer.setTracks(listOf(null, null, null))
            fixture.peer.renegotiateLater()
            runCurrent()
            fixture.observer.onIceCandidate(candidate(ufrag = "first", port = 5002))
            fixture.observer.onIceCandidate(candidate(ufrag = "restart"))
            runCurrent()
            assertEquals(1, fixture.descriptions().size)
            fixture.complete(sdp = sdp("restart"))
            runCurrent()
            assertEquals(2, fixture.descriptions().size)
            assertEquals(sdp("restart"), fixture.descriptions().last().description.sdp)
            assertTrue(fixture.ice().isEmpty())
            fixture.observer.onIceCandidate(candidate(ufrag = "first", port = 5003))
            fixture.observer.onIceCandidate(candidate(ufrag = "restart", port = 5004))
            runCurrent()
            assertEquals(1, fixture.ice().size)
        } finally { fixture.peer.close() }
    }

    @Test
    fun `closing during gathering cancels the wait without reading disposed SDP`() = runTest {
        val fixture = PeerFixture(this)
        val start = async { fixture.peer.start() }
        runCurrent()
        fixture.peer.close()
        start.join()
        fixture.observer.onIceCandidate(candidate())
        fixture.observer.onIceGatheringChange(PeerConnection.IceGatheringState.COMPLETE)
        advanceTimeBy(NATIVE_MEDIA_ICE_GATHERING_DEADLINE_MS)
        runCurrent()
        assertTrue(fixture.signals().isEmpty())
        verify(exactly = 0) { fixture.connection.localDescription }
    }

    @Test
    fun `operation cancellation cannot publish buffered or delayed candidates`() = runTest {
        val fixture = PeerFixture(this)
        try {
            val start = async { fixture.peer.start() }
            runCurrent()
            fixture.observer.onIceCandidate(candidate())
            runCurrent()
            start.cancelAndJoin()
            fixture.complete()
            fixture.observer.onIceCandidate(candidate(port = 5001))
            advanceTimeBy(NATIVE_MEDIA_ICE_GATHERING_DEADLINE_MS)
            runCurrent()
            assertTrue(fixture.signals().isEmpty())
            verify(exactly = 0) { fixture.connection.localDescription }
        } finally { fixture.peer.close() }
    }

    @Test
    fun `snapshot boundary retains unembedded candidates and respects each m-line`() {
        val ice = NativeMediaLocalIce()
        assertFalse(ice.candidate(candidate()))
        assertFalse(ice.candidate(candidate(port = 5001)))
        assertFalse(ice.candidate(candidate(mline = 1)))
        val late = ice.publish(sdp())
        assertEquals(listOf(5001, 5000), late.map { it.sdp.split(' ')[5].toInt() })
        assertEquals(listOf(0, 1), late.map { it.sdpMLineIndex })
        assertFalse(ice.candidate(candidate()))
        assertFalse(ice.candidate(candidate(port = 5001)))
        ice.cancel()
        assertFalse(ice.candidate(candidate(port = 5002)))
    }

    @Test
    fun `pending candidate capacity drops excess even when buffered duplicates are embedded`() {
        val ice = NativeMediaLocalIce()
        repeat(NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT) {
            assertFalse(ice.candidate(candidate()))
        }
        assertFalse(ice.candidate(candidate(port = 5001)))
        assertTrue(ice.publish(sdp()).isEmpty())
        // The excess callback was not retained; a genuinely later callback can
        // still use the remaining advertised budget after publication.
        assertTrue(ice.candidate(candidate(port = 5002)))
        ice.begin()
        assertFalse(ice.candidate(candidate(ufrag = "restart", port = 5003)))
        assertEquals(1, ice.publish(sdp("restart")).size)
    }

    @Test
    fun `embedded and continually gathered candidates share a bounded generation budget`() {
        val ice = NativeMediaLocalIce()
        assertTrue(ice.publish(sdp()).isEmpty())
        repeat(NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT - 1) { index ->
            assertTrue(ice.candidate(candidate(port = 5001 + index)))
        }
        assertFalse(ice.candidate(candidate(port = 5000 + NATIVE_MEDIA_LOCAL_CANDIDATE_LIMIT)))
        assertFalse(ice.candidate(candidate()))
        ice.begin()
        assertTrue(ice.publish(sdp("restart")).isEmpty())
        assertTrue(ice.candidate(candidate(ufrag = "restart", port = 6000)))
        assertFalse(ice.candidate(candidate(ufrag = "first", port = 6001)))
    }

    private class PeerFixture(scope: CoroutineScope, offerer: Boolean = true) {
        val connection = mockk<PeerConnection>(relaxed = true)
        lateinit var observer: PeerConnection.Observer
        var gathering = PeerConnection.IceGatheringState.GATHERING
        var current: SessionDescription? = null
        private var remote: SessionDescription? = null
        private val events = mutableListOf<MediaBackendEvent>()
        val peer: NativeMediaPeer

        init {
            val core = mockk<WebRtcCore>()
            val factory = mockk<PeerConnectionFactory>()
            val slots = List(3) { index ->
                mockk<RtpTransceiver>(relaxed = true).also { slot ->
                    every { slot.mediaType } returns if (index == 0) MediaStreamTrack.MediaType.MEDIA_TYPE_AUDIO else MediaStreamTrack.MediaType.MEDIA_TYPE_VIDEO
                    every { slot.sender.setTrack(any(), false) } returns true
                    every { slot.setDirection(any()) } returns true
                    every { slot.currentDirection } returns RtpTransceiver.RtpTransceiverDirection.INACTIVE
                    every { slot.receiver.track() } returns null
                }
            }
            every { core.peerConnectionFactory } returns factory
            every { factory.createPeerConnection(any<PeerConnection.RTCConfiguration>(), any<PeerConnection.Observer>()) } answers {
                observer = secondArg()
                connection
            }
            every { connection.addTransceiver(any<MediaStreamTrack.MediaType>(), any<RtpTransceiver.RtpTransceiverInit>()) } returnsMany slots
            every { connection.transceivers } returns slots
            every { connection.signalingState() } returns PeerConnection.SignalingState.STABLE
            every { connection.iceGatheringState() } answers { gathering }
            every { connection.localDescription } answers { current }
            every { connection.remoteDescription } answers { remote }
            every { connection.createOffer(any(), any<MediaConstraints>()) } answers {
                firstArg<SdpObserver>().onCreateSuccess(SessionDescription(SessionDescription.Type.OFFER, "v=0\r\n"))
            }
            every { connection.createAnswer(any(), any<MediaConstraints>()) } answers {
                firstArg<SdpObserver>().onCreateSuccess(SessionDescription(SessionDescription.Type.ANSWER, "v=0\r\n"))
            }
            every { connection.setLocalDescription(any(), any()) } answers {
                current = secondArg()
                firstArg<SdpObserver>().onSetSuccess()
            }
            every { connection.setRemoteDescription(any(), any()) } answers {
                remote = secondArg()
                firstArg<SdpObserver>().onSetSuccess()
            }
            peer = NativeMediaPeer(core, emptyList(), offerer, scope, { events.add(it) },
                ByteArray(32), ByteArray(32), {}, {})
            peer.setTracks(listOf(null, null, null))
        }

        fun complete(sdp: String = sdp(), type: SessionDescription.Type = SessionDescription.Type.OFFER) {
            current = SessionDescription(type, sdp)
            gathering = PeerConnection.IceGatheringState.COMPLETE
            observer.onIceGatheringChange(gathering)
        }

        fun signals() = events.filter { it is MediaBackendEvent.Description || it is MediaBackendEvent.IceCandidate }
        fun descriptions() = events.filterIsInstance<MediaBackendEvent.Description>()
        fun ice() = events.filterIsInstance<MediaBackendEvent.IceCandidate>()
    }

    companion object {
        private fun candidate(ufrag: String = "first", port: Int = 5000, mline: Int = 0) = IceCandidate(
            mline.toString(), mline, "candidate:1 1 udp 1234 192.0.2.1 $port typ host generation 0 ufrag $ufrag network-cost 10",
        )

        private fun sdp(ufrag: String = "first") = "v=0\r\n" +
            "a=ice-ufrag:$ufrag\r\n" +
            "m=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=mid:0\r\n" +
            "a=candidate:1 1 udp 1234 192.0.2.1 5000 typ host generation 0\r\n" +
            "m=video 9 UDP/TLS/RTP/SAVPF 96\r\na=mid:1\r\n"
    }
}
