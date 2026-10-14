package io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.dataOrNull
import io.paritytech.polkadotapp.common.presentation.loading.isLoading
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundProgressState
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundStep
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun RecoverStep(
    step: BackupFoundStep.Recover,
    username: LoadingState<String?>,
    onRecoverAction: () -> Unit,
    onOverrideAction: () -> Unit,
    progressState: BackupFoundProgressState
) {
    RecoverStepContent(
        step = step,
        username = username,
        onRecoverAction = onRecoverAction,
        onOverrideAction = onOverrideAction,
        progressState = progressState
    )
}

@Composable
private fun RecoverStepContent(
    step: BackupFoundStep.Recover,
    username: LoadingState<String?>,
    onRecoverAction: () -> Unit,
    onOverrideAction: () -> Unit,
    progressState: BackupFoundProgressState
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large)
    ) {
        NovaText(
            text = stringResource(RCommon.string.backup_found_title),
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraMedium }

        NovaText(
            text = stringResource(RCommon.string.backup_found_description),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraLarge }

        NovaText(
            text = stringResource(RCommon.string.backup_found_date_created),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary
        )

        VerticalSpacer { small }

        val timeFormatter = LocalTimeFormatter.current
        val usernameValue = username.dataOrNull
        val dateTimeText = remember(step.backupCreatedAt, usernameValue) {
            val dateTime = timeFormatter.formatShortDateTime(step.backupCreatedAt)
            if (usernameValue != null) "$dateTime ($usernameValue)" else dateTime
        }
        val isActionInProgress = progressState.isActionInProgress()

        Row(verticalAlignment = Alignment.CenterVertically) {
            NovaText(
                text = dateTimeText,
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary
            )
            if (username.isLoading()) {
                NovaCircularProgressIndicator(
                    modifier = Modifier
                        .padding(start = PolkadotTheme.spacings.small)
                        .size(16.dp),
                    color = PolkadotTheme.colors.fg.secondary,
                    strokeWidth = 2.dp
                )
            }
        }

        VerticalSpacer { extraLargeIncreased }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.backup_found_recover_action),
            style = PolkadotButtonStyle.primary(),
            onClick = onRecoverAction,
            loading = progressState == BackupFoundProgressState.RECOVERING,
            enabled = isActionInProgress.not()
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.backup_found_remove_action),
            style = PolkadotButtonStyle.ghost(),
            onClick = onOverrideAction,
            enabled = isActionInProgress.not()
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
            RecoverStep(
                step = BackupFoundStep.Recover(
                    backupCreatedAt = System.currentTimeMillis()
                ),
                username = LoadingState.Loaded("alice123"),
                onRecoverAction = {},
                onOverrideAction = {},
                progressState = BackupFoundProgressState.IDLE
            )
        }
    }
}

private fun BackupFoundProgressState.isActionInProgress(): Boolean =
    this == BackupFoundProgressState.RECOVERING || this == BackupFoundProgressState.OVERRIDING
