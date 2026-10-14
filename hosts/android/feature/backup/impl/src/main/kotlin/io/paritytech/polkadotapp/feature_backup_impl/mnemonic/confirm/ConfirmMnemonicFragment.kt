package io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_backup_impl.mnemonic.confirm.compose.ConfirmMnemonicScreen

@AndroidEntryPoint
class ConfirmMnemonicFragment : BaseComposeFragment<ConfirmMnemonicViewModel>() {
    override val viewModel: ConfirmMnemonicViewModel by viewModels()

    @Composable
    override fun Screen() = ConfirmMnemonicScreen(viewModel)
}
