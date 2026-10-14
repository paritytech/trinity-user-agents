package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.ThemeScreen
import javax.inject.Inject

@AndroidEntryPoint
class ThemeFragment : BaseComposeFragment<ThemeViewModel>() {
    override val viewModel: ThemeViewModel by viewModels()

    @Inject
    lateinit var textMessageDrawer: TextMessageDrawer

    @Composable
    override fun Screen() {
        ThemeScreen(viewModel = viewModel, textMessageDrawer)
    }
}
