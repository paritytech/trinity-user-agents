package io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.BackupFoundViewModel
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.BackupFoundScreen

@AndroidEntryPoint
class BackupFoundBottomSheet : BaseComposeBottomSheet<BackupFoundViewModel>() {
    override val viewModel: BackupFoundViewModel by viewModels()

    @Composable
    override fun Screen() = BackupFoundScreen(viewModel)
}
