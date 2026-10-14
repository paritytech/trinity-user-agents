package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.compose

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.progress.NovaLinearProgressIndicator
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.SpaBrowserPageState
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.SpaBrowserUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.SpaBrowserViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SpaBrowserScreen(viewModel: SpaBrowserViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val webView by viewModel.webView.collectAsStateWithLifecycle()

    BackHandler {
        viewModel.onBackPressed()
    }

    SpaBrowserScreenInternal(
        state = state,
        webView = webView,
    )
}

@Composable
internal fun SpaBrowserScreenInternal(
    state: SpaBrowserUiState,
    webView: WebView?,
) {
    // No chrome: the product fills the screen. There is no in-screen way to close it — the only exit is
    // the global tab bar (pull it out to switch or manage apps).
    PolkadotSurface(color = PolkadotTheme.colors.bg.surface.main) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            DotNsLoadProgressBar(progress = state.loadProgress)
            PolkadotSurface(
                modifier = Modifier.weight(1f),
                color = PolkadotTheme.colors.bg.surface.container,
            ) {
                SpaBrowserPageContent(
                    pageState = state.pageState,
                    webView = webView,
                )
            }
        }
    }
}

/**
 * Thin progress bar shown under the top bar while the `.dot` content loads. The phases map onto
 * fixed regions of the bar so it only ever fills forward:
 * - **Resolving** animates `0 → [RESOLVE_BAND_END]` over [BAND_ANIM_MILLIS]ms,
 * - **Downloading** tracks bytes across the middle band `[RESOLVE_BAND_END, DOWNLOAD_BAND_END]`,
 * - **Unpacking** animates `[DOWNLOAD_BAND_END] → 1` over [BAND_ANIM_MILLIS]ms.
 *
 * Hidden when idle / completed / failed.
 */
@Composable
private fun DotNsLoadProgressBar(progress: DotNsLoadProgress, modifier: Modifier = Modifier) {
    val target = when (progress) {
        DotNsLoadProgress.Resolving -> RESOLVE_BAND_END
        is DotNsLoadProgress.Downloading ->
            progress.fraction?.let { RESOLVE_BAND_END + (DOWNLOAD_BAND_END - RESOLVE_BAND_END) * it } ?: RESOLVE_BAND_END
        DotNsLoadProgress.Unpacking -> 1f
        DotNsLoadProgress.Idle, DotNsLoadProgress.Completed, is DotNsLoadProgress.Failed -> 0f
    }

    val animationSpec: AnimationSpec<Float> = if (progress is DotNsLoadProgress.Downloading) {
        spring(stiffness = Spring.StiffnessLow)
    } else {
        tween(BAND_ANIM_MILLIS)
    }

    val animatedFraction by animateFloatAsState(
        targetValue = target,
        animationSpec = animationSpec,
        label = "dotNsLoadFraction",
    )

    val isLoading = progress is DotNsLoadProgress.Resolving ||
        progress is DotNsLoadProgress.Downloading ||
        progress is DotNsLoadProgress.Unpacking

    if (isLoading) {
        NovaLinearProgressIndicator(
            progress = animatedFraction,
            modifier = modifier.fillMaxWidth(),
            color = PolkadotTheme.colors.fg.link
        )
    }
}

@Composable
private fun SpaBrowserPageContent(
    pageState: SpaBrowserPageState,
    webView: WebView?,
) {
    when (pageState) {
        SpaBrowserPageState.Content -> ProductWebViewHost(
            modifier = Modifier.fillMaxSize(),
            webView = webView,
        )

        SpaBrowserPageState.NoAppSurface -> DefaultErrorState(
            text = stringResource(RCommon.string.spa_browser_page_no_app_surface),
        )

        is SpaBrowserPageState.Failed -> DefaultErrorState(
            text = stringResource(pageState.error.userMessage()),
        )
    }
}

private const val RESOLVE_BAND_END = 0.1f
private const val DOWNLOAD_BAND_END = 0.9f
private const val BAND_ANIM_MILLIS = 300

@StringRes
private fun ProductResolutionError.userMessage(): Int = when (this) {
    ProductResolutionError.MalformedManifest -> RCommon.string.product_resolution_error_malformed_manifest
    ProductResolutionError.Unknown -> RCommon.string.product_resolution_error_unknown
}

@Preview
@Composable
private fun SpaBrowserScreenPreview() {
    PolkadotTheme {
        SpaBrowserScreenInternal(
            state = SpaBrowserUiState(
                title = "Web3 Summit App",
                subtitle = "web3summit.dot",
                loadProgress = DotNsLoadProgress.Downloading(0.4f),
            ),
            webView = null,
        )
    }
}
