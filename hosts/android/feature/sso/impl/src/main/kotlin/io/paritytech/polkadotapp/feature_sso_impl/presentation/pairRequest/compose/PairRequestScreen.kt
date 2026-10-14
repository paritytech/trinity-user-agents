package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.ConnectingStep
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestContract
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestDeviceUiModel
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestUiState
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components.ConfirmationContent
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components.ConnectingContent
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components.LimitReachedContent
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PairRequestScreen(contract: PairRequestContract) {
    val state by contract.state.collectAsState()

    PairRequestScreenInternal(
        state = state,
        stallReport = { contract.stalenessReport.DisplayReport() },
        onApproveClicked = contract::onApproveClicked,
        onRejectClicked = contract::onRejectClicked
    )
}

@Composable
private fun PairRequestScreenInternal(
    state: LoadingState<PairRequestUiState>,
    stallReport: @Composable () -> Unit,
    onApproveClicked: () -> Unit,
    onRejectClicked: () -> Unit,
) {
    NovaBottomSheetSurface {
        AnimatedContent(
            targetState = state,
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC },
            contentKey = { it::class },
            label = "PairRequestStateTransition"
        ) { targetState ->
            when (targetState) {
                is LoadingState.Loading -> LoadingContent()
                is LoadingState.Error -> ErrorContent(onRejectClicked)
                is LoadingState.Loaded -> when (val uiState = targetState.data) {
                    is PairRequestUiState.Confirmation -> ConfirmationContent(
                        device = uiState.device,
                        onApproveClicked = onApproveClicked,
                        onRejectClicked = onRejectClicked
                    )

                    is PairRequestUiState.Connecting -> ConnectingContent(
                        device = uiState.device,
                        step = uiState.step,
                        stallReport = stallReport,
                        onCancelClicked = onRejectClicked
                    )

                    is PairRequestUiState.LimitReached -> LimitReachedContent(
                        totalSlots = uiState.totalSlots,
                        onCloseClicked = onRejectClicked
                    )
                }
            }
        }
    }
}

@Composable
private fun LoadingContent() {
    LoadingScreenState(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.extraLarge),
    )
}

@Composable
private fun ErrorContent(onRejectClicked: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraLarge)
    ) {
        NovaText(
            text = stringResource(RCommon.string.sso_pair_request_failed_title),
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center
        )

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.pair_request_close_action),
            style = PolkadotButtonStyle.ghost(),
            onClick = onRejectClicked
        )
    }
}

@Preview
@Composable
private fun PairRequestConfirmationPreview() {
    PolkadotTheme {
        PairRequestScreenInternal(
            state = LoadingState.Loaded(
                PairRequestUiState.Confirmation(
                    device = PairRequestDeviceUiModel(
                        name = "Polkadot Desktop",
                        hostVersion = "0.2.13",
                        platformType = "MacBook Pro 16",
                    )
                )
            ),
            stallReport = {},
            onApproveClicked = {},
            onRejectClicked = {}
        )
    }
}

@Preview
@Composable
private fun PairRequestConnectingPreview() {
    PolkadotTheme {
        PairRequestScreenInternal(
            state = LoadingState.Loaded(
                PairRequestUiState.Connecting(
                    device = PairRequestDeviceUiModel(
                        name = "Polkadot Desktop",
                        hostVersion = "0.2.13",
                        platformType = "MacBook Pro 16",
                    ),
                    step = ConnectingStep.REGISTERING,
                )
            ),
            stallReport = {},
            onApproveClicked = {},
            onRejectClicked = {}
        )
    }
}

@Preview
@Composable
private fun PairRequestLimitReachedPreview() {
    PolkadotTheme {
        PairRequestScreenInternal(
            state = LoadingState.Loaded(PairRequestUiState.LimitReached(totalSlots = 8)),
            stallReport = {},
            onApproveClicked = {},
            onRejectClicked = {}
        )
    }
}
