package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.common.utils.toPayloadBundle
import io.paritytech.polkadotapp.feature_backup_api.presentation.BackupConflictPayload
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose.BackupConflictScreen
import javax.inject.Inject

@AndroidEntryPoint
class BackupConflictBottomSheet : BaseComposeBottomSheet<BackupConflictViewModel>() {
    companion object {
        const val REQUEST_KEY = "21e00a81-7068-472b-85d8-99297e061a0f"

        fun createBundle(payload: BackupConflictPayload) = payload.toPayloadBundle()
    }

    override val viewModel: BackupConflictViewModel by viewModels()

    @Inject
    lateinit var timeFormatter: TimeFormatter

    @Composable
    override fun Screen() {
        CompositionLocalProvider(
            LocalTimeFormatter provides timeFormatter
        ) {
            BackupConflictScreen(viewModel)
        }
    }
}
