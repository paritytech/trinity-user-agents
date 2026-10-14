package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.ConnectingStep
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestDeviceUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ConnectingContent(
    device: PairRequestDeviceUiModel,
    step: ConnectingStep,
    stallReport: @Composable () -> Unit,
    onCancelClicked: () -> Unit,
) {
    PairRequestDialogColumn(verticalArrangement = Arrangement.spacedBy(48.dp)) {
        PairRequestDialogTitle(stringResource(RCommon.string.pair_request_connecting_title))

        PairRequestDeviceHeader(device = device)

        Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)) {
            ProgressCard(currentStep = step)

            stallReport()
        }

        PairRequestTwoActionRow(
            cancelEnabled = false,
            primaryEnabled = false,
            primaryLoading = true,
            primaryText = stringResource(RCommon.string.pair_request_link_action),
            onCancelClicked = onCancelClicked,
            onPrimaryClicked = {},
        )
    }
}

@Composable
private fun ProgressCard(currentStep: ConnectingStep) {
    PairRequestInfoCard {
        ConnectingStep.entries.forEach { step ->
            ProgressItem(
                text = stringResource(step.titleRes),
                done = currentStep.ordinal > step.ordinal,
            )
        }
    }
}

@Composable
private fun ProgressItem(text: String, done: Boolean) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        if (done) {
            NovaIcon(
                modifier = Modifier.size(16.dp),
                imageVector = NovaIcons.Check,
                tint = PolkadotTheme.colors.fg.success
            )
        } else {
            NovaText(
                text = "•",
                color = PolkadotTheme.colors.fg.secondary,
                style = PolkadotTheme.typography.body.medium
            )
        }

        HorizontalSpacer { small }

        NovaText(
            modifier = Modifier.weight(1f),
            text = text,
            color = PolkadotTheme.colors.fg.secondary,
            style = PolkadotTheme.typography.body.medium
        )
    }
}

private val ConnectingStep.titleRes: Int
    get() = when (this) {
        ConnectingStep.VERIFYING -> RCommon.string.pair_request_progress_verifying
        ConnectingStep.REGISTERING -> RCommon.string.pair_request_progress_registering
        ConnectingStep.SYNCING -> RCommon.string.pair_request_progress_syncing
    }
