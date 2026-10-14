package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun BackupInProgressContent(
    onSecretPhraseAction: () -> Unit
) {
    BackupStatusStateContent(
        image = {
            NovaCircularProgressIndicator(
                modifier = Modifier.size(48.dp)
            )
        },
        header = stringResource(RCommon.string.settings_backup_in_progress_header),
        description = null,
        footerContent = {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.settings_backup_in_progress_action),
                enabled = false,
                onClick = {}
            )

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.settings_backup_secret_phrase_action),
                style = PolkadotButtonStyle.secondary(),
                onClick = onSecretPhraseAction
            )
        }
    )
}
