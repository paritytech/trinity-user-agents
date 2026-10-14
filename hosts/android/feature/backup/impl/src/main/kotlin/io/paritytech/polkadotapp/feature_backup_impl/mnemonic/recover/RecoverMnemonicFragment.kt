package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.recover.compose.RecoverMnemonicScreen

@AndroidEntryPoint
class RecoverMnemonicFragment : BaseComposeFragment<RecoverMnemonicViewModel>() {
    override val viewModel: RecoverMnemonicViewModel by viewModels()

    @Composable
    override fun Screen() = RecoverMnemonicScreen(viewModel)
}
