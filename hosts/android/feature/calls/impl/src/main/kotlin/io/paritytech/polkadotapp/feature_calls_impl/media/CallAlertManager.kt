package io.paritytech.polkadotapp.feature_calls_impl.media

import android.Manifest
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.RingtoneManager
import android.net.Uri
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import androidx.annotation.RequiresPermission
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_impl.R
import timber.log.Timber
import javax.inject.Inject

sealed interface CallAlert {
    data object None : CallAlert
    data object IncomingRinging : CallAlert
    data object OutgoingRingback : CallAlert
}

// Mutated only by CallService's single call-state observer (including its onCompletion teardown),
// so the player and vibrator fields need no synchronization.
class CallAlertManager @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private val ringtoneFocusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
        .setAudioAttributes(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                .build()
        )
        .build()

    private val ringbackUri: Uri = Uri.parse("android.resource://${context.packageName}/${R.raw.call_ringback}")

    private var currentAlert: CallAlert = CallAlert.None
    private var ringtonePlayer: MediaPlayer? = null
    private var ringbackPlayer: MediaPlayer? = null
    private var vibrator: Vibrator? = null

    fun setActiveAlert(alert: CallAlert) {
        if (alert == currentAlert) return
        Timber.i("CallAlertManager: $currentAlert -> $alert")
        stopCurrent()
        currentAlert = alert
        when (alert) {
            CallAlert.None -> Unit
            CallAlert.IncomingRinging -> startIncomingRinging()
            CallAlert.OutgoingRingback -> startOutgoingRingback()
        }
    }

    fun release() = setActiveAlert(CallAlert.None)

    private fun startIncomingRinging() {
        when (audioManager.ringerMode) {
            AudioManager.RINGER_MODE_SILENT -> Unit
            AudioManager.RINGER_MODE_VIBRATE -> startVibration()
            else -> {
                startRingtone()
                startVibration()
            }
        }
    }

    private fun startRingtone() {
        val ringtoneUri = RingtoneManager.getActualDefaultRingtoneUri(context, RingtoneManager.TYPE_RINGTONE)
            ?: RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE)
            ?: return

        audioManager.requestAudioFocus(ringtoneFocusRequest)
        ringtonePlayer = runCatching {
            MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                        .build()
                )
                setDataSource(context, ringtoneUri)
                isLooping = true
                prepare()
                start()
            }
        }.onFailure {
            Timber.w(it, "Failed to start ringtone")
            audioManager.abandonAudioFocusRequest(ringtoneFocusRequest)
        }.getOrNull()
    }

    @RequiresPermission(Manifest.permission.VIBRATE)
    private fun startVibration() {
        val vib = systemVibrator()?.takeIf { it.hasVibrator() } ?: return
        vib.vibrate(
            VibrationEffect.createWaveform(
                RING_VIBRATION_PATTERN, RING_VIBRATION_REPEAT_INDEX
            )
        )
        vibrator = vib
    }

    // A MediaPlayer is a registered player, unlike ToneGenerator's native track: without an active
    // VOICE_COMMUNICATION player or recorder AudioService drops MODE_IN_COMMUNICATION and the
    // communication route 6 s after they were requested, re-routing whatever is playing.
    private fun startOutgoingRingback() {
        ringbackPlayer = runCatching {
            MediaPlayer().apply {
                setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
                        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
                        .build()
                )
                setDataSource(context, ringbackUri)
                isLooping = true
                prepare()
                start()
            }
        }.onFailure { Timber.w(it, "Failed to start ringback tone") }.getOrNull()
    }

    @RequiresPermission(Manifest.permission.VIBRATE)
    private fun stopCurrent() {
        ringtonePlayer?.let { player ->
            runCatching { player.stop() }
            player.release()
            audioManager.abandonAudioFocusRequest(ringtoneFocusRequest)
        }
        ringtonePlayer = null

        vibrator?.cancel()
        vibrator = null

        ringbackPlayer?.let { player ->
            runCatching { player.stop() }
            player.release()
        }
        ringbackPlayer = null
    }

    private fun systemVibrator(): Vibrator? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            (context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as? VibratorManager)?.defaultVibrator
        } else {
            @Suppress("DEPRECATION")
            context.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator
        }

    private companion object {
        const val RING_VIBRATION_REPEAT_INDEX = 0
        val RING_VIBRATION_PATTERN = longArrayOf(0, 1000, 1000)
    }
}

// Ringback waits for the call route so it starts on the selected device, already in MODE_IN_COMMUNICATION.
internal fun ActiveCallState?.toCallAlert(audio: CallAudioDevicesState): CallAlert = when {
    isIncomingRinging() -> CallAlert.IncomingRinging
    isOutgoingRingback() && audio.isRouteApplied -> CallAlert.OutgoingRingback
    else -> CallAlert.None
}

private fun ActiveCallState?.isIncomingRinging() =
    this != null && direction == CallDirection.Incoming && status == CallStatus.Ringing

private fun ActiveCallState?.isOutgoingRingback() =
    this != null && direction == CallDirection.Outgoing &&
        (status == CallStatus.Requesting || status == CallStatus.Ringing)
