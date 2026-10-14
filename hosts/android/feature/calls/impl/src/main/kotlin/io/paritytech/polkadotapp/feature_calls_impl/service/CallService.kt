package io.paritytech.polkadotapp.feature_calls_impl.service

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder
import androidx.core.app.ServiceCompat
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAlertManager
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioRouteController
import io.paritytech.polkadotapp.feature_calls_impl.media.CallWakeLockManager
import io.paritytech.polkadotapp.feature_calls_impl.media.toCallAlert
import io.paritytech.polkadotapp.feature_calls_impl.models.CallParams
import io.paritytech.polkadotapp.feature_calls_impl.notification.CallNotificationManager
import io.paritytech.polkadotapp.feature_calls_impl.state.CallStateHolder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionReference
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.requestSessionEnabled
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject
import kotlin.coroutines.CoroutineContext

@AndroidEntryPoint
class CallService : Service(), CoroutineScope {
    companion object {
        private const val BUNDLE_KEY = "32a0bfa1-953b-4ac5-be88-26ea3b792267"
        private const val KEY_ENABLED = "io.paritytech.polkadotapp.extra.ENABLED"
        private const val ACTION_START_CALL = "io.paritytech.polkadotapp.action.START_CALL"
        private const val ACTION_INIT_INCOMING_CALL = "io.paritytech.polkadotapp.action.INCOMING_CALL"
        private const val ACTION_DECLINE_CALL = "io.paritytech.polkadotapp.action.DECLINE_CALL"
        private const val ACTION_DECLINE_WHEN_BUSY = "io.paritytech.polkadotapp.action.DECLINE_INCOMING_OFFER"
        private const val ACTION_END_CALL = "io.paritytech.polkadotapp.action.END_CALL"
        private const val ACTION_SET_CAMERA_ENABLED = "io.paritytech.polkadotapp.action.SET_CAMERA_ENABLED"
        private const val ACTION_SET_MICROPHONE_ENABLED = "io.paritytech.polkadotapp.action.SET_MICROPHONE_ENABLED"
        private const val ACTION_SELECT_AUDIO_DEVICE = "io.paritytech.polkadotapp.action.SELECT_AUDIO_DEVICE"
        private const val KEY_DEVICE_ID = "io.paritytech.polkadotapp.extra.DEVICE_ID"

        private const val CALL_NOTIFICATION_ID = 999

        fun startCallIntent(context: Context, params: CallParams): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_START_CALL)
                .putExtra(BUNDLE_KEY, params)
        }

        fun incomingCallIntent(context: Context, params: CallParams): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_INIT_INCOMING_CALL)
                .putExtra(BUNDLE_KEY, params)
        }

        fun endCallIntent(context: Context): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_END_CALL)
        }

        fun declineCallIntent(context: Context): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_DECLINE_CALL)
        }

        fun declineWhenBusyIntent(context: Context, params: CallParams): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_DECLINE_WHEN_BUSY)
                .putExtra(BUNDLE_KEY, params)
        }

        fun setCameraEnabledIntent(context: Context, enabled: Boolean): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_SET_CAMERA_ENABLED)
                .putExtra(KEY_ENABLED, enabled)
        }

        fun setMicrophoneEnabledIntent(context: Context, enabled: Boolean): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_SET_MICROPHONE_ENABLED)
                .putExtra(KEY_ENABLED, enabled)
        }

        fun selectAudioDeviceIntent(context: Context, deviceId: Int): Intent {
            return Intent(context, CallService::class.java)
                .setAction(ACTION_SELECT_AUDIO_DEVICE)
                .putExtra(KEY_DEVICE_ID, deviceId)
        }
    }

    override val coroutineContext: CoroutineContext = Dispatchers.IO + SupervisorJob()

    @Inject
    lateinit var callStateHolder: CallStateHolder

    @Inject
    lateinit var callSessionManager: CallSessionManager

    @Inject
    lateinit var contactChatSessionRefCounter: ContactChatSessionRefCounter

    @Inject
    lateinit var callWakeLockManager: CallWakeLockManager

    @Inject
    lateinit var callAlertManager: CallAlertManager

    @Inject
    lateinit var callAudioRouteController: CallAudioRouteController

    private var sessionRef: ContactChatSessionReference? = null
    private var acquireSessionRefJob: Job? = null

    private var incomingCallCanceledJob: Job? = null

    private val callNotificationManager by lazy { CallNotificationManager(this) }

    override fun onCreate() {
        super.onCreate()

        callWakeLockManager.observe()
        observeCallSounds()
    }

    private fun observeCallSounds() {
        combine(
            callStateHolder.observeActiveCall(),
            callAudioRouteController.audioDevices,
        ) { call, audio -> call.toCallAlert(audio) }
            .distinctUntilChanged()
            .onEach(callAlertManager::setActiveAlert)
            .onCompletion { callAlertManager.release() }
            .launchIn(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Timber.i("CallService received intent with action: ${intent?.action}")

        when (intent?.action) {
            ACTION_INIT_INCOMING_CALL -> {
                val callParams = intent.getParcelableExtra<CallParams>(BUNDLE_KEY)

                if (callParams == null) {
                    Timber.w("INIT_INCOMING_CALL: callParams is null, stopping service")
                    stopSelf()
                    return START_NOT_STICKY
                }

                Timber.i("INIT_INCOMING_CALL: offerId=${callParams.offerId}, chatId=${callParams.chatId}")

                if (!ensureContactSessionHeld(callParams, startId)) return START_NOT_STICKY

                ServiceCompat.startForeground(
                    this,
                    CALL_NOTIFICATION_ID,
                    callNotificationManager.createIncomingCallNotification(callParams),
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL
                )
                Timber.i("INIT_INCOMING_CALL: foreground started with PHONE_CALL")

                callStateHolder.initCall(
                    chatId = callParams.computeChatId(),
                    offerId = callParams.offerId,
                    direction = CallDirection.Incoming,
                    initiatedWithVideo = callParams.withVideo,
                )
                Timber.i("INIT_INCOMING_CALL: call state initialized")

                incomingCallCanceledJob = callSessionManager.observeIncomingCallCanceled(
                    chatId = callParams.computeChatId(),
                    offerId = callParams.offerId,
                    onCanceled = ::stopSelf
                )
                Timber.i("INIT_INCOMING_CALL: observing close signal")
            }

            ACTION_START_CALL -> {
                val callParams = intent.getParcelableExtra<CallParams>(BUNDLE_KEY)

                if (callParams == null) {
                    Timber.w("START_CALL: callParams is null, stopping service")
                    stopSelf()
                    return START_NOT_STICKY
                }

                Timber.i("START_CALL: direction=${callParams.direction}, offerId=${callParams.offerId}")

                if (!ensureContactSessionHeld(callParams, startId)) return START_NOT_STICKY

                when (callParams.direction) {
                    CallDirection.Outgoing -> startOutgoingCall(callParams)
                    CallDirection.Incoming -> acceptIncomingCall(callParams)
                }
            }

            ACTION_DECLINE_CALL -> declineCall()
            ACTION_DECLINE_WHEN_BUSY -> {
                val callParams = intent.getParcelableExtra<CallParams>(BUNDLE_KEY)
                if (callParams == null) {
                    Timber.w("DECLINE_INCOMING_OFFER: callParams is null, ignoring")
                } else {
                    declineIncomingOffer(callParams)
                }
            }
            ACTION_END_CALL -> endCall()
            ACTION_SET_CAMERA_ENABLED -> setCameraEnabled(intent.getBooleanExtra(KEY_ENABLED, true))
            ACTION_SET_MICROPHONE_ENABLED -> setMicrophoneEnabled(intent.getBooleanExtra(KEY_ENABLED, true))
            ACTION_SELECT_AUDIO_DEVICE -> selectAudioDevice(intent.getIntExtra(KEY_DEVICE_ID, 0))
        }

        return START_NOT_STICKY
    }

    private fun ensureContactSessionHeld(callParams: CallParams, startId: Int): Boolean {
        val accountId = callParams.computeChatId().contactOrNull()?.contactAccountId ?: run {
            Timber.w("CallService: chatId is not a contact chat; aborting call setup")
            stopSelf(startId)
            return false
        }

        if (acquireSessionRefJob == null) {
            acquireSessionRefJob = launch {
                sessionRef = contactChatSessionRefCounter.requestSessionEnabled(
                    accountId,
                    "ActiveCall:${callParams.offerId}"
                )
            }
        }

        return true
    }

    override fun onDestroy() {
        Timber.i("onDestroy: releasing resources, activeCall=${callStateHolder.getActiveCall()}")

        val acquireJob = acquireSessionRefJob
        acquireSessionRefJob = null
        CoroutineScope(Dispatchers.IO + NonCancellable).launch {
            acquireJob?.join()
            sessionRef?.release()
            sessionRef = null
        }

        cancel()

        callSessionManager.endSession()
        callWakeLockManager.release()
        callStateHolder.clear()

        Timber.i("onDestroy: cleanup complete")
        super.onDestroy()
    }

    private fun startOutgoingCall(callParams: CallParams) {
        Timber.i("startOutgoingCall: offerId=${callParams.offerId}")
        val chatId = callParams.computeChatId()

        ServiceCompat.startForeground(
            this,
            CALL_NOTIFICATION_ID,
            callNotificationManager.createOngoingCallNotification(callParams.callerName),
            ongoingCallForegroundType(callParams.withVideo)
        )
        Timber.i("startOutgoingCall: foreground started with ${foregroundTypeLabel(callParams.withVideo)}")

        callStateHolder.initCall(chatId, callParams.offerId, callParams.direction, callParams.withVideo)

        callSessionManager.startSession(
            sessionScope = childScope(true),
            chatId = chatId,
            offerId = callParams.offerId,
            callDirection = callParams.direction,
            withVideo = callParams.withVideo,
            onTerminated = ::stopSelf,
        )
        Timber.i("startOutgoingCall: session started")
    }

    private fun acceptIncomingCall(callParams: CallParams) {
        Timber.i("acceptIncomingCall: offerId=${callParams.offerId}, activeCall=${callStateHolder.getActiveCall()}")
        val chatId = callParams.computeChatId()

        ServiceCompat.startForeground(
            this,
            CALL_NOTIFICATION_ID,
            callNotificationManager.createOngoingCallNotification(callParams.callerName),
            ongoingCallForegroundType(callParams.withVideo)
        )
        Timber.i("acceptIncomingCall: foreground changed to ${foregroundTypeLabel(callParams.withVideo)}")

        // we don't need to listen for cancel signals separately anymore,
        // manager is already observing everything we need after the call has been accepted
        incomingCallCanceledJob?.cancel()
        incomingCallCanceledJob = null

        callStateHolder.updateStatus(CallStatus.Connecting)
        Timber.i("acceptIncomingCall: status updated, activeCall=${callStateHolder.getActiveCall()}")

        callSessionManager.startSession(
            sessionScope = childScope(true),
            chatId = chatId,
            offerId = callParams.offerId,
            callDirection = callParams.direction,
            withVideo = callParams.withVideo,
            onTerminated = ::stopSelf,
        )
        Timber.i("acceptIncomingCall: session started")
    }

    private fun declineCall() = launch {
        Timber.i("declineCall")
        callSessionManager.sendCloseSignal()
        callStateHolder.updateStatus(CallStatus.Ended)
        stopSelf()
    }

    private fun declineIncomingOffer(params: CallParams) = launch {
        Timber.i("declineIncomingOffer: offerId=${params.offerId}")
        callSessionManager.declineOffer(params.offerId, params.computeChatId())
    }

    private fun endCall() = launch {
        Timber.i("endCall")
        callSessionManager.sendCloseSignal()
        callStateHolder.updateStatus(CallStatus.Ended)
        stopSelf()
    }

    private fun setCameraEnabled(enabled: Boolean) = launch {
        Timber.i("setCameraEnabled: enabled=$enabled")
        callSessionManager.setLocalCameraEnabled(enabled)
    }

    private fun setMicrophoneEnabled(enabled: Boolean) = launch {
        Timber.i("setMicrophoneEnabled: enabled=$enabled")
        callSessionManager.setLocalMicrophoneEnabled(enabled)
    }

    private fun selectAudioDevice(deviceId: Int) {
        Timber.i("selectAudioDevice: deviceId=$deviceId")
        callSessionManager.selectAudioDevice(deviceId)
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun CallParams.computeChatId(): ChatId {
        return ChatId.fromRawValue(this.chatId)
    }

    private fun ongoingCallForegroundType(withVideo: Boolean): Int {
        val baseType = ServiceInfo.FOREGROUND_SERVICE_TYPE_PHONE_CALL or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        return if (withVideo) baseType or ServiceInfo.FOREGROUND_SERVICE_TYPE_CAMERA else baseType
    }

    private fun foregroundTypeLabel(withVideo: Boolean): String {
        return if (withVideo) "PHONE_CALL|MICROPHONE|CAMERA" else "PHONE_CALL|MICROPHONE"
    }
}
