package io.paritytech.polkadotapp.feature_calls_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

interface CallController {
    fun initiateCall(chatId: ChatId, callerName: String, withVideo: Boolean)
    fun acceptCall(chatId: ChatId, offerId: OfferId, callerName: String, withVideo: Boolean)
    suspend fun initiateIncomingCall(chatId: ChatId, offerId: OfferId, callerName: String, withVideo: Boolean)
    fun openOngoingCallScreen()
    fun declineCall()
    fun endCall()
    fun setCameraEnabled(enabled: Boolean)
    fun setMicrophoneEnabled(enabled: Boolean)
    fun selectAudioDevice(deviceId: Int)
}
