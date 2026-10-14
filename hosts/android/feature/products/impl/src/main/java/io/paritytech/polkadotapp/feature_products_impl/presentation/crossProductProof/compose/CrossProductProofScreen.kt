package io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof.CrossProductProofUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.crossProductProof.CrossProductProofViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CrossProductProofScreen(viewModel: CrossProductProofViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    CrossProductProofScreenInternal(
        state = state,
        onApprove = viewModel::onApproveClicked,
        onReject = viewModel::onRejectClicked,
    )
}

@Composable
private fun CrossProductProofScreenInternal(
    state: CrossProductProofUiState,
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
                text = stringResource(
                    RCommon.string.product_cross_product_proof_title,
                    state.callingProduct,
                    state.onBehalfOf,
                ),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary,
            )

            VerticalSpacer { mediumIncreased }

            Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)) {
                DetailRow(RCommon.string.product_cross_product_proof_on_behalf_of, state.onBehalfOf)
                DetailRow(RCommon.string.product_cross_product_proof_suffix, state.suffix)
                DetailRow(RCommon.string.product_cross_product_proof_message, state.message)
            }

            VerticalSpacer { extraLarge }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.product_cross_product_proof_approve),
                style = PolkadotButtonStyle.secondary(),
                onClick = onApprove,
            )

            VerticalSpacer { small }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.product_cross_product_proof_reject),
                style = PolkadotButtonStyle.ghost(),
                onClick = onReject,
            )
        }
    }
}

@Composable
private fun DetailRow(labelRes: Int, value: String) {
    NovaText(
        text = stringResource(labelRes, value),
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.secondary,
    )
}

@Preview
@Composable
private fun CrossProductProofScreenPreview() {
    PolkadotTheme {
        CrossProductProofScreenInternal(
            state = CrossProductProofUiState(
                callingProduct = "voting.dot",
                onBehalfOf = "identity.dot",
                suffix = "period/3",
                message = "0xdeadbeef",
            ),
            onApprove = {},
            onReject = {},
        )
    }
}
