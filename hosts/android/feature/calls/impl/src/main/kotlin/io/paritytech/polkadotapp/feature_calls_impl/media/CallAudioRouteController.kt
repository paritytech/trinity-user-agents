package io.paritytech.polkadotapp.feature_calls_impl.media

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import javax.inject.Singleton

private val BUILTIN_TYPES = setOf(CallAudioDeviceType.Earpiece, CallAudioDeviceType.Speaker)

// Singleton so the call Service (writer) and the call ViewModel (reader) share one route state.
@Singleton
class CallAudioRouteController @Inject constructor(
    private val callAudioManager: CallAudioManager,
) {
    private val userSelectedDeviceId = MutableStateFlow<Int?>(null)

    private val audioDevicesState = MutableStateFlow(CallAudioDevicesState.EMPTY)
    val audioDevices: StateFlow<CallAudioDevicesState> = audioDevicesState.asStateFlow()

    context(scope: CoroutineScope)
    fun start(defaultToSpeaker: Boolean) {
        userSelectedDeviceId.value = null
        audioDevicesState.value = CallAudioDevicesState.EMPTY
        callAudioManager.enableCallMode()

        // A newly connected device auto-wins until the user picks one; the pick is then sticky.
        combine(
            callAudioManager.observeAudioDevices(),
            userSelectedDeviceId,
        ) { devices, selectedId -> devices to resolveTarget(devices, selectedId, defaultToSpeaker) }
            .onEach { (devices, target) ->
                target ?: return@onEach
                callAudioManager.selectDevice(target)
                audioDevicesState.value = CallAudioDevicesState(devices, target.id)
            }
            .launchIn(scope)
    }

    fun selectDevice(deviceId: Int) {
        userSelectedDeviceId.value = deviceId
    }

    fun setMicrophoneMute(muted: Boolean) {
        callAudioManager.setMicrophoneMute(muted)
    }

    fun stop() {
        callAudioManager.disableCallMode()
        audioDevicesState.value = CallAudioDevicesState.EMPTY
    }

    private fun resolveTarget(
        devices: List<CallAudioDevice>,
        selectedId: Int?,
        defaultToSpeaker: Boolean,
    ): CallAudioDevice? {
        val picked = selectedId?.let { id -> devices.firstOrNull { it.id == id } }
        return picked ?: defaultDevice(devices, defaultToSpeaker)
    }

    private fun defaultDevice(devices: List<CallAudioDevice>, defaultToSpeaker: Boolean): CallAudioDevice? {
        devices.firstOrNull { it.type !in BUILTIN_TYPES }?.let { return it }
        val preferred = if (defaultToSpeaker) CallAudioDeviceType.Speaker else CallAudioDeviceType.Earpiece
        return devices.firstOrNull { it.type == preferred } ?: devices.firstOrNull()
    }
}
