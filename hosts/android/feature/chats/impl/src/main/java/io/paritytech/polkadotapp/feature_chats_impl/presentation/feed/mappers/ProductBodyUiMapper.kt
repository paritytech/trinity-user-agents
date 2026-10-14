package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.mappers

import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionsLayoutModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMediaItemModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ProductMessageBody
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductMessageStatusModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ChatResourceResolver
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.ResolvedChatImage
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import javax.inject.Inject

/** Maps the bodies of Chat Extension v2 that the native message kinds cannot hold (02-DESIGN §6). */
class ProductBodyUiMapper @Inject constructor(
    private val resourceResolver: ChatResourceResolver,
) {
    suspend fun map(
        message: ChatMessage,
        body: ProductMessageBody,
        direction: ChatMessageUiModel.Direction,
        status: ChatMessageUiModel.Status,
        replyPreview: ReplyPreview?,
        reactions: ImmutableList<ChatMessageUiModel.Reaction>,
        isEdited: Boolean,
    ): ChatMessageUiModel? = when (body) {
        is ProductMessageBody.Actions -> ChatMessageUiModel.Actions(
            id = message.id,
            timestamp = message.timestamp,
            direction = direction,
            status = status,
            origin = message.origin,
            text = body.text?.takeIf { it.isNotBlank() },
            buttons = body.actions.map { ChatMessageUiModel.Actions.ActionButton(it.actionId, it.title) }.toImmutableList(),
            layout = when (body.layout) {
                ChatActionsLayoutModel.COLUMN -> ChatMessageUiModel.Actions.ActionsLayout.COLUMN
                ChatActionsLayoutModel.GRID -> ChatMessageUiModel.Actions.ActionsLayout.GRID
            },
            reactions = reactions,
        )

        is ProductMessageBody.RichText -> if (body.media.isEmpty()) {
            body.text?.takeIf { it.isNotBlank() }?.let { text ->
                ChatMessageUiModel.Text(
                    id = message.id,
                    timestamp = message.timestamp,
                    direction = direction,
                    status = status,
                    origin = message.origin,
                    text = text,
                    replyPreview = replyPreview,
                    reactions = reactions,
                    isEdited = isEdited,
                    isMarkdown = true,
                )
            }
        } else {
            ChatMessageUiModel.ProductMedia(
                id = message.id,
                timestamp = message.timestamp,
                direction = direction,
                status = status,
                origin = message.origin,
                text = body.text?.takeIf { it.isNotBlank() },
                media = body.media.map { it.toUi() }.toImmutableList(),
                replyPreview = replyPreview,
                reactions = reactions,
                isEdited = isEdited,
            )
        }
    }

    private suspend fun ChatMediaItemModel.toUi(): ChatMessageUiModel.ProductMedia.MediaItem = when (this) {
        is ChatMediaItemModel.Image -> ChatMessageUiModel.ProductMedia.MediaItem.Image(
            model = resourceResolver.resolve(resource)
                .logFailure("Failed to resolve a product image")
                .getOrNull()
                ?.model,
            width = meta.width,
            height = meta.height,
            blurHash = meta.placeholder,
            alt = meta.alt,
        )

        is ChatMediaItemModel.Video -> ChatMessageUiModel.ProductMedia.MediaItem.Video(
            url = (resourceResolver.resolve(resource).getOrNull() as? ResolvedChatImage.Remote)?.url,
            width = meta.width,
            height = meta.height,
            blurHash = meta.placeholder,
            alt = meta.alt,
        )

        is ChatMediaItemModel.File -> ChatMessageUiModel.ProductMedia.MediaItem.File(
            fileName = fileName,
            size = meta.sizeBytes?.bytes,
            mimeType = meta.mimeType,
            alt = meta.alt,
        )
    }
}

/** The ticks of 02-DESIGN §6.5: clock, one tick, two ticks, two strong ticks, warning. */
fun ProductMessageStatusModel.toUi(): ChatMessageUiModel.Status = when (this) {
    ProductMessageStatusModel.PREPARING -> ChatMessageUiModel.Status.PENDING
    ProductMessageStatusModel.SENT -> ChatMessageUiModel.Status.SENT
    ProductMessageStatusModel.DELIVERED -> ChatMessageUiModel.Status.DELIVERED
    ProductMessageStatusModel.READ -> ChatMessageUiModel.Status.READ
    ProductMessageStatusModel.FAILED -> ChatMessageUiModel.Status.FAILED
}
