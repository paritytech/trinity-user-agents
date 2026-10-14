package io.paritytech.polkadotapp.feature_chats_impl.presentation.bot

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.MessageDrawingContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.presentation.common.getMaxMessageWidth
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.defaultAlignment
import kotlinx.serialization.Serializable

@Serializable
data class EchoContent(
    val originalText: String,
    val watchTime: Int
)

/**
 * Renderer for echo messages that displays the echoed text with
 * a distinctive visual style to showcase custom rendering.
 */
object EchoMessageRenderer : CustomChatMessageRenderer<EchoContent> {
    override val id: String = "EchoMessageRenderer"

    override val contentSerializer = EchoContent.serializer()

    @Composable
    override fun DrawMessage(
        message: ChatMessageUiModel.Custom<EchoContent>,
        context: MessageDrawingContext
    ) {
        Box(context.messageModifier) {
            Column(modifier = Modifier.align(message.direction.defaultAlignment)) {
                EchoMessageBubble(
                    content = message.content,
                    grouping = context.grouping
                )
            }
        }
    }

    override suspend fun formatNotificationContent(
        message: ChatMessage.Content.Custom<EchoContent>
    ): Result<String> {
        return message.content.map {
            "Echo: \"${it.originalText}\""
        }
    }

    @Composable
    override fun formatChatPreview(message: LastMessageUiModel.Custom<EchoContent>): Result<String> {
        return message.content.map {
            "Echo: \"${it.originalText}\""
        }
    }
}

private val AccentColor = Color(0xFF6366F1)

@Composable
private fun EchoMessageBubble(
    content: Result<EchoContent>,
    grouping: ChatMessageGrouping
) {
    val full = PolkadotTheme.radii.mediumIncreased
    val tail = PolkadotTheme.radii.small
    val shape = RoundedCornerShape(
        topStart = if (grouping.isTopAttached) tail else full,
        topEnd = full,
        bottomStart = if (grouping.isBottomAttached) tail else full,
        bottomEnd = full
    )

    Box(
        modifier = Modifier
            .widthIn(max = getMaxMessageWidth())
            .clip(shape)
            .background(PolkadotTheme.colors.bg.surface.nested)
            .border(PolkadotTheme.borders.medium, AccentColor, shape)
            .padding(PolkadotTheme.spacings.extraMedium)
    ) {
        content.fold(
            onSuccess = { echo ->
                Column {
                    Text(
                        text = "🔊 Echo",
                        style = PolkadotTheme.typography.body.small,
                        fontWeight = FontWeight.Bold,
                        color = AccentColor
                    )

                    Text(
                        text = "\"${echo.originalText}\"",
                        style = PolkadotTheme.typography.body.large,
                        fontStyle = FontStyle.Italic,
                        color = PolkadotTheme.colors.fg.primary,
                        modifier = Modifier.padding(top = PolkadotTheme.spacings.tiny)
                    )

                    Text(
                        text = "You watched this message for total of ${echo.watchTime} seconds",
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.secondary,
                        modifier = Modifier.padding(top = PolkadotTheme.spacings.tiny)
                    )
                }
            },
            onFailure = {
                Text(
                    text = "Failed to decode echo message",
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.error
                )
            }
        )
    }
}
