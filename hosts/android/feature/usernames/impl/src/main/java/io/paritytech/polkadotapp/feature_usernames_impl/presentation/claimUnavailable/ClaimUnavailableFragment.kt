package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claimUnavailable

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claimUnavailable.compose.ClaimUnavailableScreen

@AndroidEntryPoint
class ClaimUnavailableFragment : BaseComposeFragment<ClaimUnavailableViewModel>() {
    override val viewModel: ClaimUnavailableViewModel by viewModels()

    @Composable
    override fun Screen() {
        ClaimUnavailableScreen(viewModel)
    }
}
