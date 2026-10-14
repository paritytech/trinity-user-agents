package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.conflict.models.BackupConflictStep
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ConflictStep(
    step: BackupConflictStep.Conflict,
    onOverrideAction: () -> Unit,
    onCancelAction: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large)
    ) {
        NovaText(
            text = stringResource(RCommon.string.settings_backup_conflict_title),
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraMedium }

        NovaText(
            text = stringResource(RCommon.string.settings_backup_conflict_description),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraLarge }

        NovaText(
            text = stringResource(RCommon.string.settings_backup_conflict_timestamp),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary
        )

        VerticalSpacer { small }

        val timeFormatter = LocalTimeFormatter.current
        NovaText(
            text = timeFormatter.formatDateTime(step.backupCreatedAt),
            style = PolkadotTheme.typography.title.medium,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraLargeIncreased }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.settings_backup_conflict_action),
            style = PolkadotButtonStyle.primary(),
            onClick = onOverrideAction
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.ghost(),
            onClick = onCancelAction
        )
    }
}

@Preview
@Composable
private fun RecoverStepPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current)
        ) {
            ConflictStep(
                step = BackupConflictStep.Conflict(System.currentTimeMillis()),
                onOverrideAction = {},
                onCancelAction = {}
            )
        }
    }
}
