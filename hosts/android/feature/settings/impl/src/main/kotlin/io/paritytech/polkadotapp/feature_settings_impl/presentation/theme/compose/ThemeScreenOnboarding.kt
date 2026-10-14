package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.ThemeViewModel

@Composable
fun ThemeScreenOnboarding(
    viewModel: ThemeViewModel,
    messageDrawer: TextMessageDrawer
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ThemeScreenContent(
        state = state,
        messageDrawer = messageDrawer,
        onThemeSelected = viewModel::onThemeSelected,
        onConfirm = viewModel::confirm
    )
}
