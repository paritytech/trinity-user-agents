package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.PolkadotBulletText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestDeviceUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ConfirmationContent(
    device: PairRequestDeviceUiModel,
    onApproveClicked: () -> Unit,
    onRejectClicked: () -> Unit,
) {
    PairRequestDialogColumn(verticalArrangement = Arrangement.spacedBy(32.dp)) {
        PairRequestDialogTitle(stringResource(RCommon.string.pair_request_confirm_title))

        PairRequestDeviceHeader(device = device)

        InfoBulletsCard()

        PairRequestTwoActionRow(
            cancelEnabled = true,
            primaryEnabled = true,
            primaryLoading = false,
            primaryText = stringResource(RCommon.string.pair_request_link_action),
            onCancelClicked = onRejectClicked,
            onPrimaryClicked = onApproveClicked,
        )
    }
}

@Composable
private fun InfoBulletsCard() {
    PairRequestInfoCard {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.pair_request_info_intro),
            color = PolkadotTheme.colors.fg.primary,
            style = PolkadotTheme.typography.body.large
        )

        VerticalSpacer { small }

        PolkadotBulletText(
            text = stringResource(RCommon.string.pair_request_info_chats),
            style = PolkadotTheme.typography.body.medium
        )
        PolkadotBulletText(
            text = stringResource(RCommon.string.pair_request_info_send_receive),
            style = PolkadotTheme.typography.body.medium
        )
        PolkadotBulletText(
            text = stringResource(RCommon.string.pair_request_info_remove),
            style = PolkadotTheme.typography.body.medium
        )
    }
}
