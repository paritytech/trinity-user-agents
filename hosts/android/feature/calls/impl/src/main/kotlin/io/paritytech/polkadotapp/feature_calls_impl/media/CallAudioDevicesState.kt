package io.paritytech.polkadotapp.feature_calls_impl.media

data class CallAudioDevicesState(
    val devices: List<CallAudioDevice>,
    val selectedId: Int?,
) {
    val selectedDevice: CallAudioDevice? get() = devices.firstOrNull { it.id == selectedId }

    val isEarpieceActive: Boolean get() = selectedDevice?.type == CallAudioDeviceType.Earpiece
    val isRouteApplied: Boolean get() = selectedId != null

    companion object {
        val EMPTY = CallAudioDevicesState(emptyList(), null)
    }
}
