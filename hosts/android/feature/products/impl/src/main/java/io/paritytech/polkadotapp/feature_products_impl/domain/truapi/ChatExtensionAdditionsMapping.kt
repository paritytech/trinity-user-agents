package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallsModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomEventModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomFeaturesModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import uniffi.truapi.ChatRoomCall
import uniffi.truapi.ChatRoomCalls
import uniffi.truapi.ChatRoomEvent
import uniffi.truapi.ChatRoomFeatures
import uniffi.truapi.ChatRoomUserState

// Maps the values of the Chat Extension v2 additions between the bindings and the chat models of the app. The
// core has bounded and screened every value before the bridge sees it, so each unsigned value fits its model.

fun ChatRoomFeatures.toModel() = ChatRoomFeaturesModel(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCallsModel(it.audio, it.video) },
    readAloud = readAloud,
    sealedAttachments = sealedAttachments,
    hopAttachments = hopAttachments,
)

fun ChatRoomFeaturesModel.toCore() = ChatRoomFeatures(
    preimageAttachments = preimageAttachments,
    search = search,
    calls = calls?.let { ChatRoomCalls(it.audio, it.video) },
    readAloud = readAloud,
    sealedAttachments = sealedAttachments,
    hopAttachments = hopAttachments,
)

fun ChatRoomCall.toModel(): ChatRoomCallModel = when (this) {
    is ChatRoomCall.Ringing -> ChatRoomCallModel.Ringing(callId, video, caller)
    is ChatRoomCall.Active -> ChatRoomCallModel.Active(callId, video, startedAt.toLong(), mediaSession?.toDataByteArray())
}

fun ChatRoomUserStateModel.toCore(roomId: String) = ChatRoomUserState(
    roomId = roomId,
    mutedUntil = mutedUntil?.toCoreMuteEnd(),
    archived = archived,
)

/** The host sends every event of [ChatRoomEventModel]. Only the core sends `Woken`. */
fun ChatRoomEventModel.toCore(): ChatRoomEvent = when (this) {
    is ChatRoomEventModel.UserStateChanged -> ChatRoomEvent.UserStateChanged(state.mutedUntil?.toCoreMuteEnd(), state.archived)
    is ChatRoomEventModel.CallRequested -> ChatRoomEvent.CallRequested(video)
    is ChatRoomEventModel.CallAnswered -> ChatRoomEvent.CallAnswered(callId, accept)
    is ChatRoomEventModel.CallOpened -> ChatRoomEvent.CallOpened(callId)
    is ChatRoomEventModel.CallEnded -> ChatRoomEvent.CallEnded(callId)
}

// The wire marks a mute without an end with the largest u64, the store with the largest Long.
private fun Long.toCoreMuteEnd(): ULong =
    if (this == ChatRoomUserStateModel.MUTED_WITHOUT_END) ULong.MAX_VALUE else coerceAtLeast(0).toULong()
