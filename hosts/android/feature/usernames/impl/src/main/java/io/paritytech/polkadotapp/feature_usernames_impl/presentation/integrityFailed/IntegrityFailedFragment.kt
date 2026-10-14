package io.paritytech.polkadotapp.feature_usernames_impl.presentation.integrityFailed

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.integrityFailed.compose.IntegrityFailedScreen

@AndroidEntryPoint
class IntegrityFailedFragment : BaseComposeFragment<IntegrityFailedViewModel>() {
    override val viewModel: IntegrityFailedViewModel by viewModels()

    @Composable
    override fun Screen() {
        IntegrityFailedScreen(viewModel)
    }
}
