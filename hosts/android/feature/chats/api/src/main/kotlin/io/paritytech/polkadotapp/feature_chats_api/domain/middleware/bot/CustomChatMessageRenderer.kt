package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import kotlinx.serialization.KSerializer

/**
 * Interface for custom message renderers that can be provided by chat bots.
 *
 * Each renderer has a unique [id] that is used to match incoming custom messages
 * with their corresponding renderer. The [contentSerializer] is used to deserialize
 * the raw byte content into the typed content object.
 *
 * @param T The type of the custom message content
 */
interface CustomChatMessageRenderer<T> {
    /**
     * Unique identifier for this renderer.
     * This ID is stored alongside the message content to identify which renderer should be used.
     */
    val id: CustomChatMessageRendererId

    /**
     * Serializer used to decode the custom message content from ByteArray.
     */
    val contentSerializer: KSerializer<T>

    /**
     * Renders the custom message content.
     *
     * @param content The result of deserializing the message content.
     *                Will be [Result.success] with the deserialized content if parsing succeeded,
     *                or [Result.failure] if deserialization failed.
     * @param context Drawing context containing styling and layout information
     */
    @Composable
    fun DrawMessage(
        message: ChatMessageUiModel.Custom<T>,
        context: MessageDrawingContext
    )

    /**
     * Formats notification content for a given custom chat message.
     *
     * @param message The custom chat message content to be formatted. This includes a renderer ID
     * and the result of deserializing the custom message content. The deserialized content is of type T.
     * @return A [Result] containing the formatted notification content as a [String] if successful,
     * or an error if the formatting fails.
     */
    suspend fun formatNotificationContent(
        message: ChatMessage.Content.Custom<T>
    ): Result<String>

    /**
     * Formats the display content for the last chat message of a custom type.
     *
     * @param message The custom last chat message model that includes renderer data and
     * the result of deserializing the custom message content.
     * @return A [Result] containing the formatted last message string if successful, or an error if formatting fails.
     */
    @Composable // Composable so we can access stringResource and other ui-level providers
    fun formatChatPreview(
        message: LastMessageUiModel.Custom<T>,
    ): Result<String>
}

/**
 * Context for drawing custom messages, containing styling and layout information.
 * New parameters can be added here without affecting renderer signatures.
 */
@Immutable
data class MessageDrawingContext(
    /**
     * Position of this message inside its visual group. Drives the bubble's inside corner radii
     * and any other group-aware visual treatment.
     */
    val grouping: ChatMessageGrouping,
    /**
     * Modifier that should be applied to the top-level message composable
     */
    val messageModifier: Modifier,
)

typealias CustomChatMessageRendererId = String
typealias CustomChatMessageRenderersById = Map<CustomChatMessageRendererId, CustomChatMessageRenderer<*>>

@Suppress("UNCHECKED_CAST")
fun CustomChatMessageRenderer<*>.asAnyRenderer(): CustomChatMessageRenderer<Any?> {
    return this as CustomChatMessageRenderer<Any?>
}
