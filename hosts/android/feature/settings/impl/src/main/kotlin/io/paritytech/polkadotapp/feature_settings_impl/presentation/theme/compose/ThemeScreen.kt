package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.ThemeViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ThemeScreen(
    viewModel: ThemeViewModel,
    messageDrawer: TextMessageDrawer
) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ThemeScreenContent(
        state = state,
        messageDrawer = messageDrawer,
        onThemeSelected = viewModel::onThemeSelected,
        topBar = {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_theme),
                navigationAction = rememberTopBarAction(viewModel::onBackClick),
                titleAlignment = TopBarTitleAlignment.Center
            )
        }
    )
}
