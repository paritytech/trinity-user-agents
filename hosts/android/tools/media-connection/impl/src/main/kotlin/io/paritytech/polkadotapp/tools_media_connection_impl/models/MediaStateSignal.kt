package io.paritytech.polkadotapp.tools_media_connection_impl.models

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
sealed interface MediaStateSignal {
    @Serializable
    @EnumIndex(0)
    class CameraEnabled(val enabled: Boolean) : MediaStateSignal

    @Serializable
    @EnumIndex(1)
    class MicrophoneEnabled(val enabled: Boolean) : MediaStateSignal
}
