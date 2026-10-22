package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomCalls
import uniffi.truapi.ChatRoomEvent
import uniffi.truapi.ChatRoomFeatures

// Maps the values of the Chat Extension v2 additions between the bindings and the chat models of the app. The
// core has bounded and screened every value before the bridge sees it, so each unsigned value fits its model.

fun ChatRoomFeatures.toModel() = ChatRoomFeaturesModel(
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    hopAttachments = hopAttachments,
)

fun ChatRoomFeaturesModel.toCore() = ChatRoomFeatures(
    calls = calls?.let { ChatRoomCalls(it.audio, it.video) },
    hopAttachments = hopAttachments,
)

fun ChatRoomCall.toModel(): ChatRoomCallModel = when (this) {
    is ChatRoomCall.Ringing -> ChatRoomCallModel.Ringing(callId, video, caller)
    is ChatRoomCall.Active -> ChatRoomCallModel.Active(callId, video, startedAt.toLong())
}

/** The host sends every event of [ChatRoomEventModel]. */
fun ChatRoomEventModel.toCore(): ChatRoomEvent = when (this) {
    is ChatRoomEventModel.CallRequested -> ChatRoomEvent.CallRequested(video, callId)
    is ChatRoomEventModel.CallAnswered -> ChatRoomEvent.CallAnswered(callId, accept)
    is ChatRoomEventModel.CallEnded -> ChatRoomEvent.CallEnded(callId)
}
