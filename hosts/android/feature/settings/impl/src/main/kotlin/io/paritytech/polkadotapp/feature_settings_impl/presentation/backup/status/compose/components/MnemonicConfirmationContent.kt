package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.LockUnlocked
import io.paritytech.polkadotapp.design.components.icon.vectors.VisibilityOnFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.WarningFilled
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MnemonicConfirmationContent(
    onConfirm: () -> Unit,
    onCancel: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.large),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        NovaText(
            text = stringResource(RCommon.string.settings_backup_recovery_phrase_warning_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary
        )

        VerticalSpacer { extraLarge }

        Column(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.small),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
        ) {
            SecurityRecommendationItem(
                icon = NovaIcons.WarningFilled,
                text = stringResource(RCommon.string.settings_backup_recovery_phrase_warning_recommendation_1)
            )

            SecurityRecommendationItem(
                icon = NovaIcons.LockUnlocked,
                text = stringResource(RCommon.string.settings_backup_recovery_phrase_warning_recommendation_2)
            )

            SecurityRecommendationItem(
                icon = NovaIcons.VisibilityOnFilled,
                text = stringResource(RCommon.string.settings_backup_recovery_phrase_warning_recommendation_3)
            )
        }

        VerticalSpacer { extraLargeIncreased }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.settings_backup_recovery_phrase_warning_proceed_action),
            onClick = onConfirm
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.secondary(),
            onClick = onCancel
        )
    }
}

@Composable
private fun SecurityRecommendationItem(
    icon: ImageVector,
    text: String
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
    ) {
        NovaIcon(
            modifier = Modifier.size(28.dp),
            imageVector = icon,
            tint = PolkadotTheme.colors.fg.primary
        )

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}

@Preview
@Composable
private fun MnemonicConfirmationContentPreview() {
    PolkadotTheme {
        MnemonicConfirmationContent(
            onConfirm = {},
            onCancel = {}
        )
    }
}
