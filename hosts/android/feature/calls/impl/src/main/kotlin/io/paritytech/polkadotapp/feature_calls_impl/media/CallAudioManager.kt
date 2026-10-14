package io.paritytech.polkadotapp.feature_calls_impl.media

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import androidx.annotation.RequiresApi
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import timber.log.Timber
import javax.inject.Inject

class CallAudioManager @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private val audioFocusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                .build()
        )
        .build()

    fun enableCallMode() {
        audioManager.requestAudioFocus(audioFocusRequest)
        audioManager.mode = AudioManager.MODE_IN_COMMUNICATION
        audioManager.isMicrophoneMute = false
    }

    fun disableCallMode() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            audioManager.clearCommunicationDevice()
        }
        audioManager.isMicrophoneMute = false
        audioManager.mode = AudioManager.MODE_NORMAL
        audioManager.abandonAudioFocusRequest(audioFocusRequest)
    }

    fun setMicrophoneMute(muted: Boolean) {
        audioManager.isMicrophoneMute = muted
    }

    fun observeAudioDevices(): Flow<List<CallAudioDevice>> = callbackFlow {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            trySend(LEGACY_DEVICES)
            awaitClose { }
            return@callbackFlow
        }

        val callback = object : AudioDeviceCallback() {
            override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>) {
                trySend(currentDevices())
            }

            override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) {
                trySend(currentDevices())
            }
        }

        trySend(currentDevices())
        audioManager.registerAudioDeviceCallback(callback, null)
        awaitClose { audioManager.unregisterAudioDeviceCallback(callback) }
    }.distinctUntilChanged()

    fun selectDevice(device: CallAudioDevice) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            @Suppress("DEPRECATION")
            audioManager.isSpeakerphoneOn = device.type == CallAudioDeviceType.Speaker
            return
        }

        val target = audioManager.availableCommunicationDevices.firstOrNull { it.id == device.id }
        when {
            target == null -> audioManager.clearCommunicationDevice()
            audioManager.setCommunicationDevice(target) -> Unit
            else -> {
                Timber.w("setCommunicationDevice failed for type=${target.type}, falling back to default routing")
                audioManager.clearCommunicationDevice()
            }
        }
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun currentDevices(): List<CallAudioDevice> =
        audioManager.availableCommunicationDevices
            .mapNotNull { it.toCallAudioDevice() }
            .distinctBy { it.identityKey() }

    private companion object {
        const val LEGACY_EARPIECE_ID = -1
        const val LEGACY_SPEAKER_ID = -2

        val LEGACY_DEVICES = listOf(
            CallAudioDevice(LEGACY_EARPIECE_ID, CallAudioDeviceType.Earpiece, null),
            CallAudioDevice(LEGACY_SPEAKER_ID, CallAudioDeviceType.Speaker, null),
        )
    }
}

// Collapses one physical device the platform exposes under several ports into a single entry:
// a BT accessory listed under both SCO and LE, or a wired headset split into separate in/out ports.
private fun CallAudioDevice.identityKey(): String = when (type) {
    CallAudioDeviceType.Earpiece -> "earpiece"
    CallAudioDeviceType.Speaker -> "speaker"
    CallAudioDeviceType.WiredHeadset -> "wired"
    else -> name?.let { "$type:$it" } ?: "id:$id"
}

@RequiresApi(Build.VERSION_CODES.S)
private fun AudioDeviceInfo.toCallAudioDevice(): CallAudioDevice? {
    val deviceType = when (type) {
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> CallAudioDeviceType.Earpiece
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> CallAudioDeviceType.Speaker
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
        AudioDeviceInfo.TYPE_BLE_HEADSET,
        AudioDeviceInfo.TYPE_BLE_SPEAKER,
        AudioDeviceInfo.TYPE_HEARING_AID -> CallAudioDeviceType.Bluetooth
        AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> CallAudioDeviceType.WiredHeadset
        AudioDeviceInfo.TYPE_USB_HEADSET,
        AudioDeviceInfo.TYPE_USB_DEVICE,
        AudioDeviceInfo.TYPE_USB_ACCESSORY -> CallAudioDeviceType.Usb
        else -> return null
    }
    val deviceName = when (deviceType) {
        CallAudioDeviceType.Earpiece, CallAudioDeviceType.Speaker -> null
        else -> productName?.toString()?.takeIf { it.isNotBlank() }
    }
    return CallAudioDevice(id, deviceType, deviceName)
}

internal val BLUETOOTH_TYPES = setOf(
    AudioDeviceInfo.TYPE_BLE_HEADSET,
    AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
)
