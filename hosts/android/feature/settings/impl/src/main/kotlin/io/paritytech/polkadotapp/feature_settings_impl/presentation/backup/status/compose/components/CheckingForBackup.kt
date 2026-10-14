package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components

import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CheckingForBackup() {
    BackupStatusStateContent(
        image = {
            NovaCircularProgressIndicator(
                modifier = Modifier.size(48.dp)
            )
        },
        header = stringResource(RCommon.string.settings_backup_checking_for_backup_header),
        description = null,
        footerContent = {}
    )
}
