package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import io.paritytech.polkadotapp.designsystem.colors.PolkadotColorsPalette
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatBackgroundRenderer

data class CustomChatAppearance(
    val backgroundRenderer: CustomChatBackgroundRenderer?,
    val bubbleStyle: ChatMessageSurfaceStyle?,
    val dateSeparatorStyle: ChatDateSeparatorStyle?,
    val toolbarPalette: PolkadotColorsPalette?,
)
