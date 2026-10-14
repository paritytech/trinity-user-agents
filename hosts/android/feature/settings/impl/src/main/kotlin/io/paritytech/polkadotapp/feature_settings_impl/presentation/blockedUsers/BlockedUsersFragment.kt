package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.compose.BlockedUsersScreen

@AndroidEntryPoint
class BlockedUsersFragment : BaseComposeFragment<BlockedUsersViewModel>() {
    override val viewModel: BlockedUsersViewModel by viewModels()

    @Composable
    override fun Screen() {
        BlockedUsersScreen(contract = viewModel)
    }
}
