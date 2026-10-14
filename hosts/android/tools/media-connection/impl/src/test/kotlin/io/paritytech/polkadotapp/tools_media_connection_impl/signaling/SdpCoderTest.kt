package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import org.junit.Assert.assertEquals
import org.junit.Test
import org.webrtc.IceCandidate

class SdpCoderTest {
    private val coder = SdpCoder()

    @Test
    fun `should encode and decode setup correctly`() {
        val sdp = """
            v=0
            o=- 12345 67890 IN IP4 0.0.0.0
            s=-
            t=0 0
            m=application 9 UDP/DTLS/SCTP webrtc-datachannel
            c=IN IP4 0.0.0.0
            a=ice-ufrag:ufrag123
            a=ice-pwd:pwd123
            a=fingerprint:sha-256 A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6
            a=setup:actpass
            a=mid:0
            a=sctp-port:5000
        """.trimIndent()

        val candidates = listOf(
            IceCandidate("0", 0, "candidate:1 1 udp 2122260223 192.168.1.1 1234 typ host")
        )

        val input = SdpCoderSetup(sdp, candidates)
        val encoded = coder.encodeSetup(input)
        val decoded = coder.decodeSetup(encoded)

        val expectedSdp = """
            v=0
            o=- 12345 67890 IN IP4 0.0.0.0
            s=-
            t=0 0
            m=application 9 UDP/DTLS/SCTP webrtc-datachannel
            c=IN IP4 0.0.0.0
            a=ice-ufrag:ufrag123
            a=ice-pwd:pwd123
            a=fingerprint:sha-256 A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6:A1:B2:C3:D4:E5:F6
            a=setup:actpass
            a=mid:0
            a=sctp-port:5000

        """.trimIndent()

        assertEquals(expectedSdp, decoded.setupSdp)

        assertEquals(1, decoded.candidates.size)
        assertEquals("candidate:1 1 UDP 2122260223 192.168.1.1 1234 typ host", decoded.candidates[0].sdp)
    }

    @Test
    fun `should encode and decode candidates list correctly`() {
        val candidates = listOf(
            IceCandidate("0", 0, "candidate:1 1 udp 2122260223 192.168.1.1 1234 typ host"),
            IceCandidate("0", 0, "candidate:2 1 tcp 123456 10.0.0.1 5678 typ srflx")
        )

        val encoded = coder.encodeCandidates(candidates)
        val decoded = coder.decodeCandidates(encoded)

        assertEquals(2, decoded.size)
        assertEquals("candidate:1 1 UDP 2122260223 192.168.1.1 1234 typ host", decoded[0].sdp)
        assertEquals("candidate:2 1 TCP 123456 10.0.0.1 5678 typ srflx", decoded[1].sdp)
    }
}
