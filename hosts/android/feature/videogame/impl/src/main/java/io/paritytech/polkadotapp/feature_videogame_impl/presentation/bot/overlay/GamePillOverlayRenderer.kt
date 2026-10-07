package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatOverlayRenderer

internal class GamePillOverlayRenderer : CustomChatOverlayRenderer {
    @Composable
    override fun DrawOverlay() {
        val viewModel = hiltViewModel<ProductGamePillOverlayViewModel>()
        val secondsLeft by viewModel.secondsLeft.collectAsStateWithLifecycle()
        secondsLeft?.let {
            GamePillBar(
                secondsLeft = it,
                onClick = viewModel::onPillClicked,
            )
        }
    }
}
