package io.paritytech.polkadotapp.app.root.presentation.root

import androidx.compose.ui.unit.Dp
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthIndicatorsModel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

interface RootContract {
    val showDevResetPrompt: StateFlow<Boolean>

    val chatOverlays: Flow<List<ChatOverlay>>
    val isOnboarded: Flow<Boolean>
    val bottomNavHeight: StateFlow<Dp>
    val chainsHealth: StateFlow<ChainHealthIndicatorsModel>
    val isNetworkStatusTooltipVisible: StateFlow<Boolean>

    fun onDevResetStartOverClick()

    fun onDevResetDismissClick()

    fun dismissNetworkStatusTooltip()
}
