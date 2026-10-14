package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CloudOn
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun BackupExistsContent(
    onSecretPhraseAction: () -> Unit
) {
    BackupStatusStateContent(
        image = {
            NovaIcon(
                modifier = Modifier.size(56.dp),
                imageVector = NovaIcons.CloudOn,
                tint = PolkadotTheme.colors.fg.primary
            )
        },
        header = stringResource(RCommon.string.settings_backup_exists_header),
        description = stringResource(RCommon.string.settings_backup_exists_description),
        footerContent = {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.settings_backup_secret_phrase_action),
                style = PolkadotButtonStyle.secondary(),
                onClick = onSecretPhraseAction
            )
        }
    )
}
