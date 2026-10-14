package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.PairRequestScreen

@AndroidEntryPoint
class PairRequestBottomSheet : BaseComposeBottomSheet<PairRequestViewModel>() {
    override val viewModel: PairRequestViewModel by viewModels()

    @Composable
    override fun Screen() = PairRequestScreen(viewModel)
}
