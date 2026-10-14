package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.compose.DeviceDetailsScreen

@AndroidEntryPoint
class DeviceDetailsFragment : BaseComposeFragment<DeviceDetailsViewModel>() {
    override val viewModel: DeviceDetailsViewModel by viewModels()

    @Composable
    override fun Screen() {
        DeviceDetailsScreen(contract = viewModel)
    }

    companion object {
        fun createBundle(payload: DeviceDetailsPayload) = payload.toPayloadBundle()
    }
}
