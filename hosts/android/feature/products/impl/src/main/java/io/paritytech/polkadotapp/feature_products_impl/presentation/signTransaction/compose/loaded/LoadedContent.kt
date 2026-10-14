package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose.loaded

import androidx.compose.animation.AnimatedContent
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetDefaults
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.SigningContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.TransactionSignUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun LoadedContent(
    state: TransactionSignUiState,
    onApproveClicked: () -> Unit,
    onRejectClicked: () -> Unit,
    onDetailsClicked: () -> Unit,
    onBackFromDetailsClicked: () -> Unit,
) {
    AnimatedContent(
        targetState = state.showingDetails,
        transitionSpec = { NovaBottomSheetDefaults.PAGE_TRANSITION_SPEC },
        label = "DetailsTransition"
    ) { showingDetails ->
        if (showingDetails) {
            val (detailsText, detailsTitle) = when (val content = state.content) {
                is SigningContent.Transaction -> content.detailsJson to stringResource(RCommon.string.sign_transaction_details_title)
                is SigningContent.RawMessage -> content.hexData to stringResource(RCommon.string.sign_raw_message_details_title)
                is SigningContent.VrfTranscript -> content.itemsText to stringResource(RCommon.string.sign_vrf_transcript_details_title)
            }

            DetailsContent(
                detailsText = detailsText,
                title = detailsTitle,
                onBackClicked = onBackFromDetailsClicked
            )
        } else {
            MainContent(
                state = state,
                onApproveClicked = onApproveClicked,
                onRejectClicked = onRejectClicked,
                onDetailsClicked = onDetailsClicked
            )
        }
    }
}
