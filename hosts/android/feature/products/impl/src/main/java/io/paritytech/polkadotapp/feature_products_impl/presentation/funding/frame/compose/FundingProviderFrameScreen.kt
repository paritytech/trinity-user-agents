package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowLeft
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingExternalPage
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingProviderFrameUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingProviderFrameViewModel
import io.paritytech.polkadotapp.common.R as RCommon

private const val FRAME_HEIGHT_FRACTION = 0.95f
private val INDICATOR_SIZE = 48.dp

@Composable
fun FundingProviderFrameScreen(viewModel: FundingProviderFrameViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val webView by viewModel.webView.collectAsStateWithLifecycle()

    val pageState = rememberFundingExternalPageState()
    val onBack = { if (!pageState.goBack()) viewModel.onBack() }

    BackHandler(onBack = onBack)

    FundingProviderFrameScreenInternal(
        state = state,
        page = viewModel.page,
        content = { modifier ->
            val page = viewModel.page
            if (page != null) {
                FundingExternalPageView(
                    modifier = modifier,
                    page = page,
                    state = pageState,
                    onLoaded = viewModel::onPageLoaded,
                    onFailed = viewModel::onPageFailed,
                    onReturned = viewModel::onPageReturned,
                    onPickFiles = viewModel::pickFiles,
                )
            } else {
                ProductWebViewHost(modifier = modifier, webView = webView)
            }
        },
        onBack = onBack,
        onClose = viewModel::onHide,
        onSentFunds = viewModel::onSentFunds,
    )
}

@Composable
private fun FundingProviderFrameScreenInternal(
    state: FundingProviderFrameUiState,
    page: FundingExternalPage?,
    content: @Composable (Modifier) -> Unit,
    onBack: () -> Unit,
    onClose: () -> Unit,
    onSentFunds: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .height(LocalWindowInfo.current.containerDpSize.height * FRAME_HEIGHT_FRACTION),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.medium),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
            ) {
                FundingCircleButton(icon = NovaIcons.ArrowLeft, onClick = onBack)

                // An external page names its origin in the host's own chrome, with a way out of it.
                if (page != null) {
                    NovaText(
                        modifier = Modifier.weight(1f),
                        text = page.origin,
                        style = PolkadotTheme.typography.body.mediumEmphasized,
                        color = PolkadotTheme.colors.fg.secondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        textAlign = TextAlign.Center,
                    )
                    FundingCircleButton(icon = NovaIcons.Close, onClick = onClose)
                }
            }

            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth(),
                contentAlignment = Alignment.Center,
            ) {
                // The page loads behind the indicator, so an external one is in place from the start.
                when {
                    state.failed -> DefaultErrorState(
                        modifier = Modifier.fillMaxSize(),
                        text = stringResource(RCommon.string.product_resolution_error_unknown),
                    )

                    state.isContentVisible || page != null -> content(Modifier.fillMaxSize())
                }
                if (!state.isContentVisible && !state.failed) {
                    NovaCircularProgressIndicator(modifier = Modifier.size(INDICATOR_SIZE))
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
