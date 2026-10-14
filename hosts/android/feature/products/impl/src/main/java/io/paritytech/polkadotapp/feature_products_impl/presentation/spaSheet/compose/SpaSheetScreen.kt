package io.paritytech.polkadotapp.feature_products_impl.presentation.spaSheet.compose

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaSheet.SpaSheetUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaSheet.SpaSheetViewModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun SpaSheetScreen(viewModel: SpaSheetViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val webView by viewModel.webView.collectAsStateWithLifecycle()

    BackHandler {
        viewModel.onBackPressed()
    }

    SpaSheetScreenInternal(
        state = state,
        webView = webView,
    )
}

@Composable
private fun SpaSheetScreenInternal(
    state: SpaSheetUiState,
    webView: WebView?,
) {
    // No chrome at all: the product owns the sheet, so it reads as a native screen rather than a page.
    NovaBottomSheetSurface {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                // Not fillMaxHeight: the incoming max height already excludes the system bars and the surface padding.
                .height(LocalWindowInfo.current.containerDpSize.height * SHEET_SCREEN_HEIGHT_FRACTION),
            contentAlignment = Alignment.Center,
        ) {
            when {
                state.isContentVisible -> ProductWebViewHost(
                    modifier = Modifier.fillMaxSize(),
                    webView = webView,
                )

                state.loadProgress is DotNsLoadProgress.Failed -> DefaultErrorState(
                    modifier = Modifier.fillMaxSize(),
                    text = stringResource(RCommon.string.product_resolution_error_unknown),
                )

                else -> DotNsLoadProgressCircle(progress = state.loadProgress)
            }
        }
    }
}

/**
 * Circular counterpart of the browser's progress bar: the phases map onto fixed arcs so the ring
 * only ever fills forward.
 * - **Idle / Resolving** animate `0 → [RESOLVE_BAND_END]` over [BAND_ANIM_MILLIS]ms,
 * - **Downloading** tracks bytes across the middle band `[RESOLVE_BAND_END, DOWNLOAD_BAND_END]`,
 * - **Unpacking** animates `[DOWNLOAD_BAND_END] → 1` over [BAND_ANIM_MILLIS]ms.
 *
 * The ring also spins throughout, so a phase that reports no byte counts — resolving, or a download
 * whose total is unknown — still reads as working rather than as a stalled arc.
 */
@Composable
private fun DotNsLoadProgressCircle(progress: DotNsLoadProgress) {
    val target = when (progress) {
        // Nothing has been requested yet, but the sheet is already waiting on the first resolve.
        DotNsLoadProgress.Idle, DotNsLoadProgress.Resolving -> RESOLVE_BAND_END

        is DotNsLoadProgress.Downloading ->
            progress.fraction?.let { RESOLVE_BAND_END + (DOWNLOAD_BAND_END - RESOLVE_BAND_END) * it } ?: RESOLVE_BAND_END

        DotNsLoadProgress.Unpacking, DotNsLoadProgress.Completed -> 1f
        is DotNsLoadProgress.Failed -> 0f
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

    val spin = rememberInfiniteTransition(label = "dotNsLoadSpin").animateFloat(
        initialValue = 0f,
        targetValue = FULL_TURN_DEGREES,
        animationSpec = infiniteRepeatable(
            animation = tween(SPIN_MILLIS, easing = LinearEasing),
            repeatMode = RepeatMode.Restart,
        ),
        label = "dotNsLoadSpinAngle",
    )

    NovaCircularProgressIndicator(
        modifier = Modifier
            .size(INDICATOR_SIZE)
            // Read in the layer block so each frame of the spin redraws without recomposing.
            .graphicsLayer { rotationZ = spin.value },
        progress = { animatedFraction },
        color = PolkadotTheme.colors.fg.link,
        trackColor = PolkadotTheme.colors.fg.tertiary,
    )
}

private const val SHEET_SCREEN_HEIGHT_FRACTION = 0.8f
private const val RESOLVE_BAND_END = 0.1f
private const val DOWNLOAD_BAND_END = 0.9f
private const val BAND_ANIM_MILLIS = 300
private const val SPIN_MILLIS = 1200
private const val FULL_TURN_DEGREES = 360f
private val INDICATOR_SIZE = 48.dp

@Preview
@Composable
private fun SpaSheetScreenPreview() {
    PolkadotTheme {
        SpaSheetScreenInternal(
            state = SpaSheetUiState(loadProgress = DotNsLoadProgress.Downloading(0.4f)),
            webView = null,
        )
    }
}
