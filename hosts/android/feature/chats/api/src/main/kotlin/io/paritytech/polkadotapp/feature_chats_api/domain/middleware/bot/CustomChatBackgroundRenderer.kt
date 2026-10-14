package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable

interface CustomChatBackgroundRenderer {
    @Composable
    fun DrawBackground()
}
