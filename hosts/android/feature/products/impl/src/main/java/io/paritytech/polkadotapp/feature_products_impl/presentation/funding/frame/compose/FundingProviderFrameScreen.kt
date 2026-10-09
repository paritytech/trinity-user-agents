package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingProviderFrameUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingProviderFrameViewModel
import io.paritytech.polkadotapp.common.R as RCommon

private const val FRAME_HEIGHT_FRACTION = 0.95f
private val INDICATOR_SIZE = 48.dp

@Composable
fun FundingProviderFrameScreen(viewModel: FundingProviderFrameViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val webView by viewModel.webView.collectAsStateWithLifecycle()

    BackHandler(onBack = viewModel::onBack)

    FundingProviderFrameScreenInternal(
        state = state,
        webView = webView,
        onBack = viewModel::onBack,
        onSentFunds = viewModel::onSentFunds,
    )
}

@Composable
private fun FundingProviderFrameScreenInternal(
    state: FundingProviderFrameUiState,
    webView: WebView?,
    onBack: () -> Unit,
    onSentFunds: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .height(LocalWindowInfo.current.containerDpSize.height * FRAME_HEIGHT_FRACTION),
        ) {
            FundingCircleButton(
                modifier = Modifier.padding(PolkadotTheme.spacings.medium),
                icon = NovaIcons.ArrowLeft,
                onClick = onBack,
            )

            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center,
            ) {
                when {
                    state.isContentVisible -> ProductWebViewHost(modifier = Modifier.fillMaxSize(), webView = webView)
                    state.failed -> DefaultErrorState(
                        modifier = Modifier.fillMaxSize(),
                        text = stringResource(RCommon.string.product_resolution_error_unknown),
                    )

                    else -> NovaCircularProgressIndicator(modifier = Modifier.size(INDICATOR_SIZE))
                }
            }

            if (state.showsSentFunds) {
                PolkadotTextButton(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(PolkadotTheme.spacings.mediumIncreased),
                    text = stringResource(RCommon.string.funding_frame_sent_funds),
                    onClick = onSentFunds,
                )
            }
        }
    }
}
