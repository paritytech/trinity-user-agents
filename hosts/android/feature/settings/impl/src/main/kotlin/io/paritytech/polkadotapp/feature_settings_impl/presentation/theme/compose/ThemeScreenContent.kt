package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components.Footer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components.TemplateMessages
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components.ThemePicker
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components.ThemeRevealBox
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components.rememberThemeRevealState
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.models.ThemeUiState

@Composable
internal fun ThemeScreenContent(
    state: ThemeUiState,
    messageDrawer: TextMessageDrawer,
    onThemeSelected: (PolkadotAppTheme) -> Unit,
    topBar: (@Composable () -> Unit)? = null,
    onConfirm: (() -> Unit)? = null
) {
    val revealState = rememberThemeRevealState()

    ThemeRevealBox(
        state = revealState,
        modifier = Modifier.fillMaxSize()
    ) {
        PolkadotSurface(
            color = PolkadotTheme.colors.bg.surface.nested
        ) {
            Column(
                modifier = Modifier.fillMaxSize()
            ) {
                topBar?.invoke()

                Box(
                    modifier = Modifier.weight(1f),
                    contentAlignment = Alignment.Center
                ) {
                    TemplateMessages(messageDrawer)
                }

                ThemePicker(
                    themes = state.availableThemes,
                    selectedTheme = state.selectedTheme,
                    onThemeSelected = { theme, center ->
                        if (theme != state.selectedTheme) {
                            revealState.reveal(center) { onThemeSelected(theme) }
                        }
                    }
                )

                VerticalSpacer { mediumIncreased }

                Footer(onConfirm)
            }
        }
    }
}
