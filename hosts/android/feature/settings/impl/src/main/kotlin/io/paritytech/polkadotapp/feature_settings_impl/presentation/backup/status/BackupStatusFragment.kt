package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.BackupConflictBottomSheet
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.BackupStatusScreen

@AndroidEntryPoint
class BackupStatusFragment : BaseComposeFragment<BackupStatusViewModel>() {
    override val viewModel: BackupStatusViewModel by viewModels()

    @Composable
    override fun Screen() {
        BackupStatusScreen(viewModel)
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        observeResult<Boolean>(BackupConflictBottomSheet.REQUEST_KEY) { backupOverridden ->
            if (backupOverridden) {
                viewModel.onBackupOverriden()
            }
        }
    }
}
