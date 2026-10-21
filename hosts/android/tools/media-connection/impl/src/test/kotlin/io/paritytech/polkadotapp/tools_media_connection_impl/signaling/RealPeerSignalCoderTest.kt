package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignal
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test
import org.webrtc.IceCandidate

class RealPeerSignalCoderTest {
    private val coder = RealPeerSignalCoder()
    private val sdpCoder = SdpCoder()

    // An offer with one candidate and a close, as the chat product encodes them (`PeerSignalsCodec`).
    private val productOffer = (
        "08006903763d300a6f3d2d20313233343520363738393020494e2049503420302e302e302e300a733d2d0a743d3020300a6d3d6170" +
            "706c69636174696f6e2039205544502f44544c532f53435450207765627274632d646174616368616e6e656c0a633d494e204950" +
            "3420302e302e302e300a613d6963652d75667261673a75667261673132330a613d6963652d7077643a7077643132330a613d6669" +
            "6e6765727072696e743a7368612d3235362041313a42320a613d73657475703a616374706173730a613d6d69643a300a613d7363" +
            "74702d706f72743a353030300a0204d863616e6469646174653a312031205544502032313232323630323233203139322e313638" +
            "2e312e3120313233342074797020686f737400000000010430"
        ).fromHex()
    private val productClosed = "0403".fromHex()

    private val setup = sdpCoder.encodeSetup(
        SdpCoderSetup(
            setupSdp = """
                v=0
                o=- 12345 67890 IN IP4 0.0.0.0
                s=-
                t=0 0
                m=application 9 UDP/DTLS/SCTP webrtc-datachannel
                c=IN IP4 0.0.0.0
                a=ice-ufrag:ufrag123
                a=ice-pwd:pwd123
                a=fingerprint:sha-256 A1:B2
                a=setup:actpass
                a=mid:0
                a=sctp-port:5000
            """.trimIndent(),
            candidates = listOf(IceCandidate("0", 0, "candidate:1 1 udp 2122260223 192.168.1.1 1234 typ host")),
        )
    )

    @Test
    fun `an offer of the engine is the product's offer and its candidate, byte for byte`() {
        assertEquals(productOffer.toHexString(), coder.encode(PeerSignal.Offer(setup)).toHexString())
        assertArrayEquals(productClosed, coder.encode(PeerSignal.Closed))
    }

    @Test
    fun `the product's offer reaches the engine with the candidate it carries`() {
        val signals = coder.decode(productOffer)

        assertEquals(1, signals.size)
        assertArrayEquals(setup, (signals.single() as PeerSignal.Offer).setup)
        assertEquals(listOf(PeerSignal.Closed), coder.decode(productClosed))
    }

    @Test
    fun `candidates on their own stay candidates`() {
        val candidates = sdpCoder.encodeCandidates(listOf(IceCandidate("0", 0, "candidate:2 1 tcp 1 10.0.0.2 9 typ srflx")))

        val signals = coder.decode(coder.encode(PeerSignal.Candidates(candidates)))

        assertArrayEquals(candidates, (signals.single() as PeerSignal.Candidates).candidates)
    }
}
