package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_chats_api.presentation.TextMessageDrawer
import io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.ThemeScreenOnboarding
import javax.inject.Inject
import kotlin.getValue

@AndroidEntryPoint
class ThemeFragmentOnboarding : BaseComposeFragment<ThemeViewModel>() {
    override val viewModel: ThemeViewModel by viewModels()

    @Inject
    lateinit var textMessageDrawer: TextMessageDrawer

    @Composable
    override fun Screen() {
        ThemeScreenOnboarding(viewModel = viewModel, textMessageDrawer)
    }
}
