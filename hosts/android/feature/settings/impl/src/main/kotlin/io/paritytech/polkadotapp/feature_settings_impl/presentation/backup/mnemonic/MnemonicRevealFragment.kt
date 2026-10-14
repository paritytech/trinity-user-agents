package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.mnemonic.compose.MnemonicRevealScreen

@AndroidEntryPoint
class MnemonicRevealFragment : BaseComposeFragment<MnemonicRevealViewModel>() {
    override val viewModel: MnemonicRevealViewModel by viewModels()

    @Composable
    override fun Screen() {
        MnemonicRevealScreen(viewModel)
    }
}
