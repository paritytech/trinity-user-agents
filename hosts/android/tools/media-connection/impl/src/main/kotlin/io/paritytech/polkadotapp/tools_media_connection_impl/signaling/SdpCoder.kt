package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.paritytech.polkadotapp.tools_media_connection_impl.models.*
import kotlinx.serialization.builtins.ListSerializer
import org.webrtc.IceCandidate
import java.math.BigInteger
import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.nio.ByteBuffer

data class SdpCoderSetup(
    val setupSdp: String,
    val candidates: List<IceCandidate>
)

class SdpCoder {
    companion object {
        const val EXPECTED_COMPONENT_ID = 1
    }

    private data class MinimalSdpBase(
        val iceUfrag: String,
        val icePwd: String,
        val fingerprint: String,
        val sdpType: String
    )

    fun encodeSetup(input: SdpCoderSetup): ByteArray {
        val minimalBase = extractMinimalBase(input.setupSdp)
        val (sessionId, sessionVersion) = extractSessionInfo(input.setupSdp)
        val fingerprintData = parseFingerprint(minimalBase.fingerprint)
        val sdpType: SdpType = if (minimalBase.sdpType == "offer") SdpType.OFFER else SdpType.ANSWER
        val minimalCandidates = input.candidates.map { parseCandidate(it) }

        val minimalSetup = MinimalSetup(
            sdpType = sdpType,
            sessionId = sessionId,
            sessionVersion = sessionVersion,
            iceUFrag = minimalBase.iceUfrag,
            icePwd = minimalBase.icePwd,
            fingerprint = fingerprintData,
            candidates = minimalCandidates
        )

        return BinaryScale.encodeToByteArray(MinimalSetup.serializer(), minimalSetup)
    }

    fun decodeSetup(data: ByteArray): SdpCoderSetup {
        val minimalSetup = BinaryScale.decodeFromByteArray<MinimalSetup>(data)

        val fingerprintString = formatFingerprint(minimalSetup.fingerprint)
        val sdpBase = reconstructSdpBase(
            sdpType = minimalSetup.sdpType,
            sessionId = minimalSetup.sessionId,
            sessionVersion = minimalSetup.sessionVersion,
            iceUfrag = minimalSetup.iceUFrag,
            icePwd = minimalSetup.icePwd,
            fingerprint = fingerprintString
        )
        val candidates = minimalSetup.candidates.map { reconstructCandidate(it) }

        return SdpCoderSetup(
            setupSdp = sdpBase,
            candidates = candidates
        )
    }

    fun encodeCandidates(candidates: List<IceCandidate>): ByteArray {
        val minimalCandidates = candidates.map { parseCandidate(it) }
        return BinaryScale.encodeToByteArray(ListSerializer(MinimalCandidate.serializer()), minimalCandidates)
    }

    fun decodeCandidates(data: ByteArray): List<IceCandidate> {
        val minimalCandidates = BinaryScale.decodeFromByteArray(ListSerializer(MinimalCandidate.serializer()), data)
        return minimalCandidates.map { reconstructCandidate(it) }
    }

    private fun extractMinimalBase(sdp: String): MinimalSdpBase {
        val lines = sdp.lines()
        var iceUfrag = ""
        var icePwd = ""
        var fingerprint = ""
        var sdpType = "offer"

        for (line in lines) {
            val trimmed = line.trim()
            when {
                trimmed.startsWith("a=ice-ufrag:") -> iceUfrag = trimmed.substringAfter("a=ice-ufrag:")
                trimmed.startsWith("a=ice-pwd:") -> icePwd = trimmed.substringAfter("a=ice-pwd:")
                trimmed.startsWith("a=fingerprint:") -> fingerprint = trimmed.substringAfter("a=fingerprint:")
                trimmed.startsWith("a=setup:") -> {
                    val setup = trimmed.substringAfter("a=setup:")
                    sdpType = if (setup == "actpass") "offer" else "answer"
                }
            }
        }

        return MinimalSdpBase(iceUfrag, icePwd, fingerprint, sdpType)
    }

    private fun extractSessionInfo(sdp: String): Pair<BigInteger, BigInteger> {
        sdp.lines().forEach { line ->
            val trimmed = line.trim()
            if (trimmed.startsWith("o=")) {
                val parts = trimmed.substring(2).split(" ").filter { it.isNotEmpty() }
                if (parts.size >= 3) {
                    val sessionId = BigInteger(parts[1])
                    val sessionVersion = BigInteger(parts[2])
                    return sessionId to sessionVersion
                }
            }
        }

        return BigInteger.ZERO to BigInteger.ZERO
    }

    private fun parseFingerprint(fingerprint: String): ByteArray {
        val trimmed = fingerprint.trim()
        val hexString = if (trimmed.contains(" ")) {
            trimmed.substringAfter(" ")
        } else {
            trimmed
        }
        val cleanHex = hexString.replace(":", "")

        return cleanHex.fromHex()
    }

    private fun formatFingerprint(data: ByteArray): String {
        return "sha-256 ${data.toColonHexString()}"
    }

    private fun parseCandidate(candidate: IceCandidate): MinimalCandidate {
        val trimmed = candidate.sdp.trim()

        val parts = trimmed.substringAfter("candidate:").split(" ").filter { it.isNotEmpty() }

        val foundation = parts[0]

        val transportType = when (parts[2].lowercase()) {
            "tcp" -> TransportType.TCP
            "udp" -> TransportType.UDP
            else -> error("Unsupported transport type: ${parts[2]}")
        }
        val priority = parts[3].toInt()
        val address = IpAddress.fromString(parts[4])
        val port = parts[5].toUShort()

        val candidateType = when (parts[7].lowercase()) {
            "host" -> CandidateType.HOST
            "srflx" -> CandidateType.SRFLX
            "relay" -> CandidateType.RELAY
            "prflx" -> CandidateType.PRFLX
            else -> error("Unsupported candidate type: ${parts[7]}")
        }

        return MinimalCandidate(foundation, priority, transportType, address, port, candidateType)
    }

    private fun reconstructCandidate(candidate: MinimalCandidate): IceCandidate {
        val transportString = when (candidate.transportType) {
            TransportType.TCP -> "TCP"
            TransportType.UDP -> "UDP"
        }
        val typString = when (candidate.candidateType) {
            CandidateType.HOST -> "host"
            CandidateType.SRFLX -> "srflx"
            CandidateType.RELAY -> "relay"
            CandidateType.PRFLX -> "prflx"
        }

        val sdp = "candidate:${candidate.foundation} $EXPECTED_COMPONENT_ID $transportString ${candidate.priority} ${candidate.address.toAddressString()} ${candidate.port} typ $typString"

        return IceCandidate("0", 0, sdp)
    }

    private fun reconstructSdpBase(
        sdpType: SdpType,
        sessionId: BigInteger,
        sessionVersion: BigInteger,
        iceUfrag: String,
        icePwd: String,
        fingerprint: String
    ): String {
        val setup = when (sdpType) {
            SdpType.OFFER -> "actpass"
            SdpType.ANSWER -> "active"
        }

        return """
        v=0
        o=- $sessionId $sessionVersion IN IP4 0.0.0.0
        s=-
        t=0 0
        m=application 9 UDP/DTLS/SCTP webrtc-datachannel
        c=IN IP4 0.0.0.0
        a=ice-ufrag:$iceUfrag
        a=ice-pwd:$icePwd
        a=fingerprint:$fingerprint
        a=setup:$setup
        a=mid:0
        a=sctp-port:5000

        """.trimIndent()
        // sdp intentionally has a newline at the end, as WebRTC requires it for correct parsing
    }
}

private fun ByteArray.toColonHexString(): String = joinToString(separator = ":") { eachByte -> "%02X".format(eachByte) }

private fun IpAddress.toAddressString(): String {
    val bytes = when (this) {
        is IpAddress.Ipv4 -> {
            byteArrayOf(comp1.toByte(), comp2.toByte(), comp3.toByte(), comp4.toByte())
        }

        is IpAddress.Ipv6 -> {
            ByteBuffer.allocate(16)
                .putShort(comp1.toShort())
                .putShort(comp2.toShort())
                .putShort(comp3.toShort())
                .putShort(comp4.toShort())
                .putShort(comp5.toShort())
                .putShort(comp6.toShort())
                .putShort(comp7.toShort())
                .putShort(comp8.toShort())
                .array()
        }
    }

    return InetAddress.getByAddress(bytes).hostAddress
}

fun IpAddress.Companion.fromString(addressString: String): IpAddress {
    return when (val inetAddress = InetAddress.getByName(addressString)) {
        is Inet4Address -> {
            val bytes = inetAddress.address
            IpAddress.Ipv4(
                bytes[0].toUByte(),
                bytes[1].toUByte(),
                bytes[2].toUByte(),
                bytes[3].toUByte()
            )
        }

        is Inet6Address -> {
            val buffer = ByteBuffer.wrap(inetAddress.address)
            IpAddress.Ipv6(
                buffer.short.toUShort(), buffer.short.toUShort(), buffer.short.toUShort(), buffer.short.toUShort(),
                buffer.short.toUShort(), buffer.short.toUShort(), buffer.short.toUShort(), buffer.short.toUShort()
            )
        }

        else -> error("Unsupported IP address format")
    }
}
