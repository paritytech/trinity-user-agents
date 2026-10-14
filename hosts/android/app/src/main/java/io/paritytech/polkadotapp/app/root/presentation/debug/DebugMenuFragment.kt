package io.paritytech.polkadotapp.app.root.presentation.debug

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.app.root.presentation.debug.compose.DebugMenuScreen
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment

@AndroidEntryPoint
class DebugMenuFragment : BaseComposeFragment<DebugMenuViewModel>() {
    override val viewModel: DebugMenuViewModel by viewModels()

    @Composable
    override fun Screen() {
        DebugMenuScreen(viewModel)
    }
}
