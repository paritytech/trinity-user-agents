package io.paritytech.polkadotapp.feature_splash_impl.presentation

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_splash_impl.presentation.compose.SplashScreen

@AndroidEntryPoint
class SplashFragment : BaseComposeFragment<SplashViewModel>() {
    override val viewModel: SplashViewModel by viewModels()

    @Composable
    override fun Screen() {
        SplashScreen(viewModel)
    }
}
