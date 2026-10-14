package io.paritytech.polkadotapp.feature_settings_impl.presentation.legalAndSupport

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.legalAndSupport.compose.LegalAndSupportScreen

@AndroidEntryPoint
class LegalAndSupportFragment : BaseComposeFragment<LegalAndSupportViewModel>() {
    override val viewModel: LegalAndSupportViewModel by viewModels()

    @Composable
    override fun Screen() {
        LegalAndSupportScreen(contract = viewModel)
    }
}
