package io.paritytech.polkadotapp.feature_calls_impl.media

data class CallAudioDevice(
    val id: Int,
    val type: CallAudioDeviceType,
    // null = no device-supplied name (builtin, or blank productName) -> UI falls back to the type label
    val name: String?,
)

enum class CallAudioDeviceType {
    Earpiece,
    Speaker,
    Bluetooth,
    WiredHeadset,
    Usb,
}
