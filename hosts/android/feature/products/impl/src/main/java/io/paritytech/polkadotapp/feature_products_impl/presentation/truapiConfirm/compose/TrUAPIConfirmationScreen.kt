package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.DetailValue
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.TrUAPIConfirmationContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.TrUAPIConfirmationDetail
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm.TrUAPIConfirmationUiState
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun TrUAPIConfirmationScreen(contract: TrUAPIConfirmationContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    state.onLoaded { data ->
        TrUAPIConfirmationScreenInternal(
            state = data,
            onApprove = contract::onApproveClicked,
            onReject = contract::onRejectClicked,
        )
    }
}

@Composable
private fun TrUAPIConfirmationScreenInternal(
    state: TrUAPIConfirmationUiState,
    onApprove: () -> Unit,
    onReject: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier.padding(
                top = PolkadotTheme.spacings.large,
                bottom = PolkadotTheme.spacings.mediumIncreased,
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
            ),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            NovaText(
                text = stringResource(state.titleRes),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
            )

            VerticalSpacer { small }

            NovaText(
                text = stringResource(RCommon.string.truapi_confirm_requested_by, state.productId),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )

            if (state.details.isNotEmpty()) {
                VerticalSpacer { mediumIncreased }

                Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny),
                ) {
                    state.details.forEach { DetailRow(it) }
                }
            }

            VerticalSpacer { extraLarge }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.truapi_confirm_approve),
                style = PolkadotButtonStyle.secondary(),
                onClick = onApprove,
            )

            VerticalSpacer { small }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.truapi_confirm_reject),
                style = PolkadotButtonStyle.ghost(),
                onClick = onReject,
            )
        }
    }
}

@Composable
private fun DetailRow(detail: TrUAPIConfirmationDetail) {
    Row(modifier = Modifier.fillMaxWidth()) {
        NovaText(
            text = stringResource(detail.labelRes),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )

        HorizontalSpacer { small }

        NovaText(
            text = when (val value = detail.value) {
                is DetailValue.Text -> value.text
                is DetailValue.Resource -> stringResource(value.res)
            },
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Preview
@Composable
private fun TrUAPIConfirmationScreenPreview() {
    PolkadotTheme {
        TrUAPIConfirmationScreenInternal(
            state = TrUAPIConfirmationUiState(
                titleRes = RCommon.string.truapi_confirm_title_create_proof,
                productId = "browse.dot",
                details = persistentListOf(
                    TrUAPIConfirmationDetail(
                        RCommon.string.truapi_confirm_label_ring,
                        DetailValue.Text("0x1234"),
                    ),
                ),
            ),
            onApprove = {},
            onReject = {},
        )
    }
}
