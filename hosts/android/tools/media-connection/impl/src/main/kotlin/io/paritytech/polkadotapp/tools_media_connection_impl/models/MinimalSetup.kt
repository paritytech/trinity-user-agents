package io.paritytech.polkadotapp.tools_media_connection_impl.models

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

@Serializable
class MinimalSetup(
    val sdpType: SdpType,
    val sessionId: BigIntegerSerializable,
    val sessionVersion: BigIntegerSerializable,
    val iceUFrag: String,
    val icePwd: String,
    val fingerprint: ByteArray,
    val candidates: List<MinimalCandidate>
)

@Serializable
class MinimalCandidate(
    val foundation: String,
    val priority: Int,
    val transportType: TransportType,
    val address: IpAddress,
    val port: UShort,
    val candidateType: CandidateType
)

@Serializable
enum class SdpType {
    OFFER,
    ANSWER,
}

@Serializable
enum class TransportType {
    TCP,
    UDP
}

@Serializable
enum class CandidateType {
    HOST,
    SRFLX,
    RELAY,
    PRFLX
}

@Serializable
sealed interface IpAddress {
    @Serializable
    @EnumIndex(0)
    class Ipv4(val comp1: UByte, val comp2: UByte, val comp3: UByte, val comp4: UByte) : IpAddress

    @Serializable
    @EnumIndex(1)
    class Ipv6(
        val comp1: UShort,
        val comp2: UShort,
        val comp3: UShort,
        val comp4: UShort,
        val comp5: UShort,
        val comp6: UShort,
        val comp7: UShort,
        val comp8: UShort,
    ) : IpAddress
}
