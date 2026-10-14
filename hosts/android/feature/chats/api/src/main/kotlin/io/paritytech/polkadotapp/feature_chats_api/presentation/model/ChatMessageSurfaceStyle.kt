package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import androidx.compose.foundation.BorderStroke
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Immutable
data class ChatMessageSurfaceStyle(
    val backgroundColor: Color,
    val border: BorderStroke? = null,
    val textColor: Color? = null,
) {
    companion object {
        @Composable
        fun default(direction: ChatMessageUiModel.Direction): ChatMessageSurfaceStyle {
            val backgroundColor = when (direction) {
                ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.bg.surface.container
                ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.bg.surface.containerInverted
            }
            return ChatMessageSurfaceStyle(backgroundColor = backgroundColor)
        }

        val Transparent = ChatMessageSurfaceStyle(backgroundColor = Color.Transparent)
    }
}
