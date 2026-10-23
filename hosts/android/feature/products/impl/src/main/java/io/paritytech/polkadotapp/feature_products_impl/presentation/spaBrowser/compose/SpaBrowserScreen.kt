package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.compose

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.*
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleStartEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.icon.vectors.Refreshing
import io.paritytech.polkadotapp.design.components.progress.NovaLinearProgressIndicator
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
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

    LifecycleStartEffect(Unit) {
        viewModel.onScreenStarted()

        onStopOrDispose { }
    }

    SpaBrowserScreenInternal(
        state = state,
        webView = webView,
        onRefresh = viewModel::onRefresh,
        onUpdateReload = viewModel::onUpdateReloadClick,
        onUpdateDismiss = viewModel::onUpdateDismissClick,
    )
}

@Composable
internal fun SpaBrowserScreenInternal(
    state: SpaBrowserUiState,
    webView: WebView?,
    onRefresh: () -> Unit,
    onUpdateReload: () -> Unit,
    onUpdateDismiss: () -> Unit,
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
                Box(modifier = Modifier.fillMaxSize()) {
                    SpaBrowserPageContent(
                        pageState = state.pageState,
                        webView = webView,
                    )

                    UpdateReadyNotice(
                        product = state.updateReadyFor?.value,
                        onReload = onUpdateReload,
                        onDismiss = onUpdateDismiss,
                        modifier = Modifier.align(Alignment.TopCenter),
                    )

                    if (state.isDevServer) {
                        PolkadotIconButton(
                            icon = NovaIcons.Refreshing,
                            onClick = onRefresh,
                            modifier = Modifier
                                .align(Alignment.BottomEnd)
                                .padding(PolkadotTheme.spacings.mediumIncreased),
                            style = PolkadotButtonStyle.secondary(),
                            size = PolkadotIconButtonSize.medium(),
                        )
                    }
                }
            }
        }
    }
}

// Floats over the page like the root's notices do: the user may be in the middle of something and decides when to reload.
@Composable
private fun UpdateReadyNotice(
    modifier: Modifier = Modifier,
    product: String?,
    onReload: () -> Unit,
    onDismiss: () -> Unit,
) {
    // The name stays on the notice while it slides out, or the text would go blank for the exit animation.
    var shownProduct by remember { mutableStateOf(product) }
    if (product != null) shownProduct = product

    AnimatedVisibility(
        modifier = modifier.padding(
            horizontal = PolkadotTheme.spacings.mediumIncreased,
            vertical = PolkadotTheme.spacings.small,
        ),
        visible = product != null,
        enter = slideInVertically(initialOffsetY = { -it }) + fadeIn(),
        exit = slideOutVertically(targetOffsetY = { -it }) + fadeOut(),
    ) {
        UpdateReadyNoticeContent(
            product = shownProduct.orEmpty(),
            onReload = onReload,
            onDismiss = onDismiss,
        )
    }
}

@Composable
private fun UpdateReadyNoticeContent(
    product: String,
    onReload: () -> Unit,
    onDismiss: () -> Unit,
) {
    val closeDescription = stringResource(RCommon.string.common_close)

    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            // A tap on the notice must not reach the page under it.
            .pointerInput(Unit) {},
        color = PolkadotTheme.colors.bg.action.tertiary,
        shape = PolkadotTheme.shapes.extraMedium,
    ) {
        Row(
            modifier = Modifier.padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.small,
                end = PolkadotTheme.spacings.extraTiny,
            ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            NovaIcon(
                modifier = Modifier.size(NoticeIconSize),
                imageVector = NovaIcons.Info,
                tint = PolkadotTheme.colors.fg.secondary,
            )

            NovaText(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.spa_browser_update_ready, product),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.primary,
            )

            PolkadotTextButton(
                text = stringResource(RCommon.string.spa_browser_update_reload),
                style = PolkadotButtonStyle.ghost(),
                size = PolkadotButtonSize.medium(),
                shape = PolkadotTheme.shapes.full,
                onClick = onReload,
            )

            PolkadotIconButton(
                modifier = Modifier.semantics { contentDescription = closeDescription },
                icon = NovaIcons.Close,
                onClick = onDismiss,
                style = PolkadotButtonStyle.ghost(),
                size = PolkadotIconButtonSize.small(),
                shape = PolkadotTheme.shapes.full,
            )
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

private val NoticeIconSize = 24.dp

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
            onUpdateReload = {},
            onUpdateDismiss = {},
            onRefresh = {},
        )
    }
}
