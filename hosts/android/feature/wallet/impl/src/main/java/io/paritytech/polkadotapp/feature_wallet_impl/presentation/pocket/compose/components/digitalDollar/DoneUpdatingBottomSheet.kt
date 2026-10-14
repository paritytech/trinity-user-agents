@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun DoneUpdatingBottomSheet(
    isVisible: Boolean,
    onDismissRequest: () -> Unit,
    onConfirm: () -> Unit
) {
    NovaModalBottomSheet(
        isVisible = isVisible,
        onDismissRequest = onDismissRequest
    ) {
        DoneUpdatingBottomSheetContent(
            onCancelClick = onDismissRequest,
            onConfirmClick = {
                onConfirm()
                onDismissRequest()
            }
        )
    }
}

@Composable
private fun DoneUpdatingBottomSheetContent(
    onCancelClick: () -> Unit,
    onConfirmClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(all = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        VerticalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.asset_details_balance_close_confirm_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.asset_details_balance_close_confirm_description),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center
        )

        VerticalSpacer { extraLarge }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.asset_details_balance_close_confirm_confirm),
            style = PolkadotButtonStyle.primary(),
            onClick = onConfirmClick,
            shape = PolkadotButtonShape.pill
        )
        VerticalSpacer { mediumIncreased }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.secondary(),
            onClick = onCancelClick,
            shape = PolkadotButtonShape.pill
        )
    }
}

@Preview
@Composable
private fun DoneUpdatingBottomSheetContentPreview() {
    PolkadotTheme {
        NovaBottomSheetSurface {
            DoneUpdatingBottomSheetContent(onCancelClick = {}, onConfirmClick = {})
        }
    }
}
