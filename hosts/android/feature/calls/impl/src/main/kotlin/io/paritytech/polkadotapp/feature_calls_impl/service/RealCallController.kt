package io.paritytech.polkadotapp.feature_calls_impl.service

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_calls_api.domain.CallController
import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.IncomingCallGate
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_impl.models.CallParams
import io.paritytech.polkadotapp.feature_calls_impl.presentation.CallActivity
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import timber.log.Timber
import java.util.UUID
import javax.inject.Inject

class RealCallController @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val callStateTracker: CallStateTracker,
    private val incomingCallGate: IncomingCallGate,
) : CallController {
    override fun initiateCall(chatId: ChatId, callerName: String, withVideo: Boolean) {
        if (!hasRecordAudioPermission()) {
            Timber.w("initiateCall: RECORD_AUDIO not granted, aborting call setup")
            return
        }

        val offerId = UUID.randomUUID().toString()

        val params = CallParams(chatId.value.value, offerId, callerName, CallDirection.Outgoing, withVideo)

        context.startForegroundService(CallService.startCallIntent(context, params))
        context.startActivity(CallActivity.newIntent(context))
    }

    override fun acceptCall(chatId: ChatId, offerId: OfferId, callerName: String, withVideo: Boolean) {
        if (!hasRecordAudioPermission()) {
            Timber.w("acceptCall: RECORD_AUDIO not granted, declining")
            context.startService(CallService.declineCallIntent(context))
            return
        }

        val params = CallParams(chatId.value.value, offerId, callerName, CallDirection.Incoming, withVideo)

        context.startForegroundService(CallService.startCallIntent(context, params))
    }

    override suspend fun initiateIncomingCall(chatId: ChatId, offerId: OfferId, callerName: String, withVideo: Boolean) {
        val params = CallParams(chatId.value.value, offerId, callerName, CallDirection.Incoming, withVideo)

        val activeCall = callStateTracker.getActiveCall()
        if (activeCall != null) {
            if (activeCall.offerId == offerId) {
                Timber.i("initiateIncomingCall: duplicate offer for active call, ignoring offerId=$offerId")
                return
            }
            Timber.i("initiateIncomingCall: already in a call, declining offerId=$offerId")
            context.startService(CallService.declineWhenBusyIntent(context, params))
            return
        }

        if (!incomingCallGate.shouldRing(chatId, offerId)) {
            Timber.i("initiateIncomingCall: gate rejected, offerId=$offerId")
            return
        }

        context.startForegroundService(CallService.incomingCallIntent(context, params))
    }

    override fun openOngoingCallScreen() {
        context.startActivity(CallActivity.newIntent(context))
    }

    override fun declineCall() {
        context.startService(CallService.declineCallIntent(context))
    }

    override fun endCall() {
        context.startService(CallService.endCallIntent(context))
    }

    override fun setCameraEnabled(enabled: Boolean) {
        context.startService(CallService.setCameraEnabledIntent(context, enabled))
    }

    override fun setMicrophoneEnabled(enabled: Boolean) {
        context.startService(CallService.setMicrophoneEnabledIntent(context, enabled))
    }

    override fun selectAudioDevice(deviceId: Int) {
        context.startService(CallService.selectAudioDeviceIntent(context, deviceId))
    }

    private fun hasRecordAudioPermission(): Boolean {
        return ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.RECORD_AUDIO
        ) == PackageManager.PERMISSION_GRANTED
    }
}
