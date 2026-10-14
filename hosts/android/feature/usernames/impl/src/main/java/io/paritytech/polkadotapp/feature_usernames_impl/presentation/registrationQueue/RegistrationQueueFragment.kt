package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.RegistrationQueueScreen

@AndroidEntryPoint
class RegistrationQueueFragment : BaseComposeFragment<RegistrationQueueViewModel>() {
    override val viewModel: RegistrationQueueViewModel by viewModels()

    @Composable
    override fun Screen() {
        RegistrationQueueScreen(viewModel)
    }
}
