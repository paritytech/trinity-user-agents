package io.paritytech.polkadotapp.feature_backup_impl.backupFound.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.compound.PolkadotCheckBox
import io.paritytech.polkadotapp.design.components.compound.polkadotColors
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_backup_impl.backupFound.models.BackupFoundProgressState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun OverrideStep(
    onOverrideAction: () -> Unit,
    onOverrideCancel: () -> Unit,
    progressState: BackupFoundProgressState
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.large)
    ) {
        Column(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large)
        ) {
            NovaText(
                text = stringResource(RCommon.string.backup_override_title),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary
            )

            VerticalSpacer { extraMedium }

            NovaText(
                text = stringResource(RCommon.string.backup_override_description),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary
            )
        }

        VerticalSpacer { 28.dp }

        val isOverriding = progressState == BackupFoundProgressState.OVERRIDING

        var isConfirmed by remember { mutableStateOf(false) }
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(
                    enabled = isOverriding.not(),
                    onClick = { isConfirmed = !isConfirmed }
                )
                .padding(
                    horizontal = PolkadotTheme.spacings.large,
                    vertical = PolkadotTheme.spacings.extraMedium
                ),
            verticalAlignment = Alignment.CenterVertically
        ) {
            PolkadotCheckBox(
                checked = isConfirmed,
                colors = CheckboxDefaults.polkadotColors(),
                onCheckedChange = { isConfirmed = !isConfirmed },
                enabled = isOverriding.not()
            )

            HorizontalSpacer { extraMedium }

            NovaText(
                text = stringResource(RCommon.string.backup_override_confirmation),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary
            )
        }

        VerticalSpacer { 20.dp }

        Column(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large)
        ) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.backup_override_continue_action),
                style = PolkadotButtonStyle.destructive(),
                onClick = onOverrideAction,
                loading = isOverriding,
                enabled = isConfirmed && isOverriding.not()
            )

            VerticalSpacer { small }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_cancel),
                style = PolkadotButtonStyle.ghost(),
                onClick = onOverrideCancel,
                enabled = isOverriding.not()
            )
        }
    }
}

@Preview
@Composable
private fun OverrideStepPreview() {
    PolkadotTheme {
        OverrideStep(
            onOverrideAction = {},
            onOverrideCancel = {},
            progressState = BackupFoundProgressState.IDLE
        )
    }
}
