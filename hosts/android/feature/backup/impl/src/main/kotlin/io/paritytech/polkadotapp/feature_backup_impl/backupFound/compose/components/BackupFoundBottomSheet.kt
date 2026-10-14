package io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.BackupFoundViewModel
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.BackupFoundScreen
import javax.inject.Inject

@AndroidEntryPoint
class BackupFoundBottomSheet : BaseComposeBottomSheet<BackupFoundViewModel>() {
    @Inject
    lateinit var timeFormatter: TimeFormatter
    override val viewModel: BackupFoundViewModel by viewModels()

    @Composable
    override fun Screen() {
        CompositionLocalProvider(LocalTimeFormatter provides timeFormatter) {
            BackupFoundScreen(viewModel)
        }
    }
}
