package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupFoundPayload
import io.paritytech.polkadotapp.feature_backup_api.presentation.RecoverOptionsPayload
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim.compose.PickUsernameScreen

@AndroidEntryPoint
class ClaimUsernameFragment : BaseComposeFragment<ClaimUsernameViewModel>() {
    override val viewModel: ClaimUsernameViewModel by viewModels()

    @Composable
    override fun Screen() {
        PickUsernameScreen(viewModel)
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        observeResult<BackupFoundPayload.Result>(BackupFoundPayload.REQUEST_KEY) { result ->
            when (result) {
                BackupFoundPayload.Result.OVERRIDDEN -> viewModel.onBackupOverridden()
                BackupFoundPayload.Result.RECOVERED -> viewModel.onImportedFromBackup()
            }
        }

        observeResult<RecoverOptionsPayload.Result>(RecoverOptionsPayload.REQUEST_KEY) { result ->
            when (result) {
                RecoverOptionsPayload.Result.IMPORTED_FROM_BACKUP -> viewModel.onImportedFromBackup()
            }
        }
    }
}
