package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.compose.LinkedDevicesScreen

@AndroidEntryPoint
class LinkedDevicesFragment : BaseComposeFragment<LinkedDevicesViewModel>() {
    override val viewModel: LinkedDevicesViewModel by viewModels()

    @Composable
    override fun Screen() {
        LinkedDevicesScreen(contract = viewModel)
    }
}
