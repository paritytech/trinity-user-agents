package io.paritytech.polkadotapp.feature_chats_impl.data.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDraftRelation
import kotlinx.serialization.Serializable

internal fun ChatDraftRelation.toScaleBytes(): ByteArray =
    BinaryScale.encodeToByteArray(ChatDraftRelationScale.serializer(), toScale())

internal fun ByteArray.toChatDraftRelation(): ChatDraftRelation =
    runCatching { BinaryScale.decodeFromByteArray(ChatDraftRelationScale.serializer(), this) }
        .logFailure("Failed to decode chat draft relation")
        .getOrDefault(ChatDraftRelationScale.None)
        .toDomain()

private fun ChatDraftRelation.toScale(): ChatDraftRelationScale = when (this) {
    ChatDraftRelation.None -> ChatDraftRelationScale.None
    is ChatDraftRelation.Reply -> ChatDraftRelationScale.Reply(messageId)
    is ChatDraftRelation.Edit -> ChatDraftRelationScale.Edit(messageId, originalText)
}

private fun ChatDraftRelationScale.toDomain(): ChatDraftRelation = when (this) {
    ChatDraftRelationScale.None -> ChatDraftRelation.None
    is ChatDraftRelationScale.Reply -> ChatDraftRelation.Reply(messageId)
    is ChatDraftRelationScale.Edit -> ChatDraftRelation.Edit(messageId, originalText)
}

@Serializable
private sealed interface ChatDraftRelationScale {
    @Serializable
    @EnumIndex(0)
    data object None : ChatDraftRelationScale

    @Serializable
    @EnumIndex(1)
    data class Reply(val messageId: ChatMessageId) : ChatDraftRelationScale

    @Serializable
    @EnumIndex(2)
    data class Edit(
        val messageId: ChatMessageId,
        val originalText: String,
    ) : ChatDraftRelationScale
}
