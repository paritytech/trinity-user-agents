package io.paritytech.polkadotapp.feature_xcm_api.multiLocation

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decoder.ScaleDecoder
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encoder.ScaleEncoder
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

@Serializable(with = MultiLocationSerializer::class)
data class RelativeMultiLocation(
    val parents: Int,
    override val interior: Interior
) : MultiLocation(interior), VersionedToDynamicScaleInstance {
    init {
        require(parents >= 0) {
            "Parents cannot be negative"
        }
    }

    override fun toEncodableInstance(xcmVersion: XcmVersion): Any {
        return toEncodableInstanceExt(xcmVersion)
    }

    override fun toString(): String {
        return "{ parents: $parents, interior: $interior }"
    }

    /**
     * Restore an absolute location using this relative location and a given POV.
     *
     * Algorithm (inverse of AbsoluteMultiLocation.fromPointOfViewOf):
     * 1) Start from POV's junctions.
     * 2) Go "up" by `parents` (drop that many items from the end).
     * 3) Append this relative interior's junctions (if any).
     */
    fun absoluteLocationViewingFrom(pov: AbsoluteMultiLocation): AbsoluteMultiLocation {
        require(parents <= pov.junctions.size) {
            """
                Invalid relative location from given pov:
                Relative location has ${this.parents} parents whereas pov has only ${pov.junctions.size} junctions
            """.trimIndent()
        }

        // 1+2) go up from POV
        val base = pov.junctions.dropLast(parents)

        // 3) append interior junctions
        val resultJunctions = base + junctions

        return AbsoluteMultiLocation(resultJunctions)
    }
}

fun RelativeMultiLocation.isHere(): Boolean {
    return parents == 0 && interior.isHere()
}

private class MultiLocationSerializer : KSerializer<RelativeMultiLocation> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor("RelativeMultiLocation", PrimitiveKind.STRING)

    override fun serialize(
        encoder: Encoder,
        value: RelativeMultiLocation
    ) {
        require(encoder is ScaleEncoder)

        encoder.encodeRaw(value.toEncodableInstance(XcmVersion.GLOBAL_DEFAULT))
    }

    override fun deserialize(decoder: Decoder): RelativeMultiLocation {
        require(decoder is ScaleDecoder)

        val raw = decoder.decodeRaw()
        return bindMultiLocation(raw)
    }
}
