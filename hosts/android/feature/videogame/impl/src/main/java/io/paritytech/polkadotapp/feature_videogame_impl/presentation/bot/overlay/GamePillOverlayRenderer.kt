package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOverlayRenderer
import kotlinx.coroutines.flow.StateFlow

internal interface GamePillViewModel {
    val pillState: StateFlow<VideoGamePillState>

    fun onPillClicked()
}

internal class GamePillOverlayRenderer(
    private val viewModel: @Composable () -> GamePillViewModel,
) : CustomChatOverlayRenderer {
    @Composable
    override fun DrawOverlay() {
        val vm = viewModel()
        val state by vm.pillState.collectAsStateWithLifecycle()
        val shown = state as? VideoGamePillState.Shown ?: return
        GamePillBar(
            state = shown,
            showChevron = true,
            onClick = vm::onPillClicked,
        )
    }
}
