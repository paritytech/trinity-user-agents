package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict

import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupConflictPayload
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose.BackupConflictScreen

@AndroidEntryPoint
class BackupConflictBottomSheet : BaseComposeBottomSheet<BackupConflictViewModel>() {
    companion object {
        const val REQUEST_KEY = "21e00a81-7068-472b-85d8-99297e061a0f"

        fun createBundle(payload: BackupConflictPayload) = payload.toPayloadBundle()
    }

    override val viewModel: BackupConflictViewModel by viewModels()

    @Composable
    override fun Screen() = BackupConflictScreen(viewModel)
}
