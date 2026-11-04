package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.EaseIn
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.lerp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ShownProduct
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.COLLECTIBLES_LIFT_DURATION_MS
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.COLLECTIBLES_LIFT_TILT_DEGREES
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCollectiblesImageSharedElement
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

private const val COVER_OPEN_DEGREES = -160f
private const val COVER_OPEN_DURATION_MS = 220
private const val COVER_EDGE_ON_DEGREES = -90f
private const val COVER_CAMERA_DISTANCE = 32f
private const val ATTACH_STALL_FRAME_MS = 120
private const val ATTACH_STALL_PATIENCE_MS = 600

private const val PAGE_EXPAND_DURATION_MS = 250
private const val SKELETON_SWEEP_LOOP_MS = 1350
private const val SKELETON_SWEEP_BAND_WIDTH = 0.5f
private const val SKELETON_SWEEP_ALPHA = 0.12f
private const val SKELETON_DOT_ALPHA = 0.25f
private const val SKELETON_DOT_COLUMNS = 12

private const val NANOS_PER_MILLI = 1_000_000L

private val PageCornerRadius = 20.dp
private val SkeletonTitleSize = DpSize(136.dp, 16.dp)
private val SkeletonSubtitleSize = DpSize(180.dp, 10.dp)
private val SkeletonButtonSize = 40.dp
private val SkeletonFirstCardHeight = 260.dp
private val SkeletonFirstLabelWidth = 80.dp
private val SkeletonSecondLabelWidth = 116.dp
private val SkeletonLabelHeight = 14.dp
private val SkeletonDotPitch = 24.dp
private val SkeletonDotRadius = 2.5.dp
private val CoverSpine = TransformOrigin(0f, 0.5f)
private val PageTop = TransformOrigin(0.5f, 0f)
private val ReadyTimeout = 10.seconds

@Composable
fun PocketCollectibles(
    product: ShownProduct?,
    saturation: () -> Float,
    onBack: () -> Unit,
) {
    BackHandler { onBack() }

    val noProgressYet = remember { MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle) }
    val noWebViewYet = remember { MutableStateFlow<WebView?>(null) }
    val loadProgress by (product?.session?.loadProgress ?: noProgressYet).collectAsStateWithLifecycle()
    val webView by (product?.session?.webView ?: noWebViewYet).collectAsStateWithLifecycle()

    val book = remember { CollectiblesBookMotion() }
    var failed by remember { mutableStateOf(false) }
    var webViewAttached by remember { mutableStateOf(false) }
    var sweeping by remember { mutableStateOf(false) }

    LaunchedEffect(book) {
        launch { book.lift() }
        delay(COLLECTIBLES_LIFT_DURATION_MS.toLong())
        book.openCover()

        val outcome = withTimeoutOrNull(ReadyTimeout) {
            snapshotFlow { loadProgress is DotNsLoadProgress.Failed || webView != null }.first { it }
            if (loadProgress !is DotNsLoadProgress.Failed) {
                // Attaching the WebView blocks the UI thread for 100-300 ms, so the sweep starts only after it.
                webViewAttached = true
                awaitAttachStall()
                sweeping = true
            }
            snapshotFlow { loadProgress }.first { it is DotNsLoadProgress.Completed || it is DotNsLoadProgress.Failed }
        }
        if (outcome is DotNsLoadProgress.Completed) book.handOff() else failed = true
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
    ) {
        PolkadotTopBar(
            title = stringResource(RCommon.string.pocket_collectibles_title),
            titleAlignment = TopBarTitleAlignment.Center
        )

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f)
                .windowInsetsPadding(LocalAppNavigationBarInsets.current)
        ) {
            CollectiblesPage(
                modifier = Modifier
                    .fillMaxSize()
                    .collectiblesPageLayer(book, inset = PolkadotTheme.spacings.mediumIncreased),
                book = book,
                webView = webView.takeIf { webViewAttached },
                sweeping = sweeping,
                failed = failed
            )

            CollectiblesSketchbook(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(PolkadotTheme.spacings.mediumIncreased)
                    .pocketCollectiblesImageSharedElement()
                    .graphicsLayer { rotationZ = book.tilt.value }
                    .graphicsLayer {
                        val angle = book.coverAngle.value
                        transformOrigin = CoverSpine
                        // At the default distance a full-width cover turning past 45° swings into the lens and fills the screen.
                        cameraDistance = COVER_CAMERA_DISTANCE * density
                        rotationY = angle
                        // A layer has no back face of its own: past edge-on it would draw the cover mirrored.
                        alpha = if (angle < COVER_EDGE_ON_DEGREES) 0f else 1f
                    },
                saturation = saturation(),
                expanded = true
            )
        }
    }
}

private suspend fun awaitAttachStall() {
    var last = withFrameNanos { it }
    val deadline = last + ATTACH_STALL_PATIENCE_MS * NANOS_PER_MILLI
    while (true) {
        val now = withFrameNanos { it }
        val stalled = now - last > ATTACH_STALL_FRAME_MS * NANOS_PER_MILLI
        last = now
        if (stalled) {
            withFrameNanos { }
            return
        }
        if (now >= deadline) return
    }
}

@Composable
private fun CollectiblesPage(
    modifier: Modifier,
    book: CollectiblesBookMotion,
    webView: WebView?,
    sweeping: Boolean,
    failed: Boolean,
) {
    Box(
        modifier = modifier.background(PolkadotTheme.colors.bg.surface.main),
        contentAlignment = Alignment.Center
    ) {
        if (failed) {
            NovaText(
                text = stringResource(RCommon.string.product_resolution_error_unknown),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )
        } else {
            CollectiblesSkeleton(modifier = Modifier.fillMaxSize(), sweeping = sweeping)

            // Laid out at full size from the start and only scaled, so the page never reflows on the way up.
            ProductWebViewHost(
                modifier = Modifier.fillMaxSize(),
                webView = webView
            )
        }
    }
}

@Composable
private fun CollectiblesSkeleton(modifier: Modifier, sweeping: Boolean) {
    Column(
        modifier = modifier
            .skeletonSweep(sweeping)
            .padding(PolkadotTheme.spacings.mediumIncreased)
    ) {
        VerticalSpacer { small }
        Box(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.align(Alignment.Center),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                SkeletonBlock(modifier = Modifier.size(SkeletonTitleSize), shape = PolkadotTheme.shapes.small)
                VerticalSpacer { small }
                SkeletonBlock(modifier = Modifier.size(SkeletonSubtitleSize), shape = PolkadotTheme.shapes.full)
            }
            SkeletonBlock(
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .size(SkeletonButtonSize),
                shape = PolkadotTheme.shapes.full
            )
        }
        VerticalSpacer { small }
        SkeletonCard(
            modifier = Modifier
                .fillMaxWidth()
                .height(SkeletonFirstCardHeight),
            labelWidth = SkeletonFirstLabelWidth
        )
        VerticalSpacer { large }
        SkeletonCard(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            labelWidth = SkeletonSecondLabelWidth
        )
    }
}

@Composable
private fun SkeletonCard(modifier: Modifier, labelWidth: Dp) {
    val dotColor = PolkadotTheme.colors.fg.tertiary.copy(alpha = SKELETON_DOT_ALPHA)

    Column(
        modifier = modifier
            .background(PolkadotTheme.colors.bg.surface.container, PolkadotTheme.shapes.extraLarge)
            .padding(PolkadotTheme.spacings.mediumIncreased)
    ) {
        SkeletonBlock(
            modifier = Modifier.size(width = labelWidth, height = SkeletonLabelHeight),
            shape = PolkadotTheme.shapes.small
        )
        VerticalSpacer { mediumIncreased }
        Box(
            modifier = Modifier
                .fillMaxSize()
                .clipToBounds()
                .drawBehind { drawDotGrid(dotColor) }
        )
    }
}

@Composable
private fun SkeletonBlock(modifier: Modifier, shape: Shape) {
    Box(modifier = modifier.background(PolkadotTheme.colors.bg.surface.nested, shape))
}

private fun DrawScope.drawDotGrid(color: Color) {
    val pitch = SkeletonDotPitch.toPx()
    val radius = SkeletonDotRadius.toPx()
    val left = (size.width - pitch * (SKELETON_DOT_COLUMNS - 1)) / 2
    var y = radius
    while (y + radius <= size.height) {
        repeat(SKELETON_DOT_COLUMNS) { column -> drawCircle(color, radius, Offset(left + column * pitch, y)) }
        y += pitch
    }
}

@Composable
private fun Modifier.skeletonSweep(running: Boolean): Modifier {
    val highlight = PolkadotTheme.colors.fg.primary.copy(alpha = SKELETON_SWEEP_ALPHA)
    val progress = remember { Animatable(0f) }
    LaunchedEffect(running) {
        if (running) progress.animateTo(1f, infiniteRepeatable(tween(SKELETON_SWEEP_LOOP_MS, easing = LinearEasing)))
    }

    return drawWithCache {
        val band = size.width * SKELETON_SWEEP_BAND_WIDTH
        val brush = Brush.horizontalGradient(listOf(Color.Transparent, highlight, Color.Transparent), endX = band)
        val bandSize = Size(band, size.height)

        onDrawWithContent {
            drawContent()
            translate(left = -band + (size.width + band) * progress.value) { drawRect(brush, size = bandSize) }
        }
    }
}

private fun Modifier.collectiblesPageLayer(book: CollectiblesBookMotion, inset: Dp): Modifier = graphicsLayer {
    val expansion = book.expansion.value
    val insetPx = inset.toPx()
    val restScale = if (size.width > 0f) (size.width - 2 * insetPx) / size.width else 1f
    val scale = lerp(restScale, 1f, expansion)

    transformOrigin = PageTop
    scaleX = scale
    scaleY = scale
    translationY = insetPx * (1f - expansion)
    alpha = book.pageAlpha.value
    // The clip is scaled with the page, so the radius is divided back out to read as the design's on screen.
    shape = RoundedCornerShape(PageCornerRadius.toPx() * (1f - expansion) / scale)
    clip = true
}

@Stable
private class CollectiblesBookMotion {
    val tilt = Animatable(0f)
    val coverAngle = Animatable(0f)
    val pageAlpha = Animatable(0f)
    val expansion = Animatable(0f)

    suspend fun lift() {
        val half = COLLECTIBLES_LIFT_DURATION_MS / 2
        tilt.animateTo(COLLECTIBLES_LIFT_TILT_DEGREES, tween(half))
        tilt.animateTo(0f, tween(half))
    }

    suspend fun openCover() = coroutineScope {
        launch { pageAlpha.animateTo(1f, tween(COVER_OPEN_DURATION_MS)) }
        coverAngle.animateTo(COVER_OPEN_DEGREES, tween(COVER_OPEN_DURATION_MS, easing = EaseIn))
    }

    suspend fun handOff() {
        expansion.animateTo(1f, tween(PAGE_EXPAND_DURATION_MS, easing = EaseOut))
    }
}
