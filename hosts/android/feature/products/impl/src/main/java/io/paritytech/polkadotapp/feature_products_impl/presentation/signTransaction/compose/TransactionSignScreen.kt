package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.SigningAccountUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.SigningContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.TransactionSignContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.TransactionSignUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose.loaded.LoadedContent

@Composable
fun TransactionSignScreen(contract: TransactionSignContract) {
    val state by contract.state.collectAsState()

    TransactionSignScreenInternal(
        state = state,
        onApproveClicked = contract::onApproveClicked,
        onRejectClicked = contract::onRejectClicked,
        onDetailsClicked = contract::onDetailsClicked,
        onBackFromDetailsClicked = contract::onBackFromDetailsClicked
    )
}

@Composable
private fun TransactionSignScreenInternal(
    state: LoadingState<TransactionSignUiState>,
    onApproveClicked: () -> Unit,
    onRejectClicked: () -> Unit,
    onDetailsClicked: () -> Unit,
    onBackFromDetailsClicked: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier
            .systemBarsPadding()
            .padding(PolkadotTheme.spacings.small),
        color = PolkadotTheme.colors.bg.surface.nested,
        shape = PolkadotTheme.shapes.extraLarge
    ) {
        AnimatedContent(
            targetState = state,
            transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC },
            contentKey = { it::class },
            label = "TransactionSignStateTransition"
        ) { targetState ->
            when (targetState) {
                is LoadingState.Loading -> LoadingContent()
                is LoadingState.Error -> ErrorContent(onRejectClicked)
                is LoadingState.Loaded -> LoadedContent(
                    state = targetState.data,
                    onApproveClicked = onApproveClicked,
                    onRejectClicked = onRejectClicked,
                    onDetailsClicked = onDetailsClicked,
                    onBackFromDetailsClicked = onBackFromDetailsClicked
                )
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

@Preview
@Composable
private fun TransactionSignScreenLoadedPreview() {
    PolkadotTheme {
        TransactionSignScreenInternal(
            state = LoadingState.Loaded(
                TransactionSignUiState(
                    requesterName = "Polkadot Portal",
                    requesterIconUrl = "https://example.com/icon.png",
                    content = SigningContent.Transaction(
                        callName = "Balances.transfer",
                        detailsJson = """{"address": "13Qbq8...", "call": {"module": "Balances", "function": "transfer"}}"""
                    ),
                    signingAccount = SigningAccountUi.Product("my-product", "0"),
                )
            ),
            onApproveClicked = {},
            onRejectClicked = {},
            onDetailsClicked = {},
            onBackFromDetailsClicked = {}
        )
    }
}

@Preview
@Composable
private fun TransactionSignScreenRawMessagePreview() {
    PolkadotTheme {
        TransactionSignScreenInternal(
            state = LoadingState.Loaded(
                TransactionSignUiState(
                    requesterName = "Polkadot Portal",
                    requesterIconUrl = "https://example.com/icon.png",
                    content = SigningContent.RawMessage(
                        hexData = "0x48656c6c6f20576f726c6421"
                    ),
                    signingAccount = SigningAccountUi.Product("my-product", "1"),
                )
            ),
            onApproveClicked = {},
            onRejectClicked = {},
            onDetailsClicked = {},
            onBackFromDetailsClicked = {}
        )
    }
}

@Preview
@Composable
private fun TransactionSignScreenLoadingPreview() {
    PolkadotTheme {
        TransactionSignScreenInternal(
            state = LoadingState.Loading,
            onApproveClicked = {},
            onRejectClicked = {},
            onDetailsClicked = {},
            onBackFromDetailsClicked = {}
        )
    }
}

@Preview
@Composable
private fun TransactionSignScreenErrorPreview() {
    PolkadotTheme {
        TransactionSignScreenInternal(
            state = LoadingState.Error(IllegalStateException("Failed to load data")),
            onApproveClicked = {},
            onRejectClicked = {},
            onDetailsClicked = {},
            onBackFromDetailsClicked = {}
        )
    }
}
