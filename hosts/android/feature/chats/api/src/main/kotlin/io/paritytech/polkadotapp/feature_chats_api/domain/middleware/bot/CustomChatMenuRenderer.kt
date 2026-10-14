package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import androidx.compose.runtime.Composable

interface CustomChatMenuRenderer {
    @Composable
    fun DrawMenu(onDismiss: () -> Unit)
}
