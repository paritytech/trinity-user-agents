package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import kotlinx.collections.immutable.ImmutableList
import kotlin.math.abs
import kotlin.math.roundToInt

/**
 * The coins, and the room the card gives them.
 *
 * Two heights, and the difference between them matters more than it looks.
 *
 * The *room* is what the card reserves. It takes the grid's height on expanding and gives it back over the
 * collapse. Growing is deliberately not animated: the coins fly out into a box that is already the right
 * size, and animating it made the scroll view chase a growing content size and bounce against its own edge.
 * Shrinking is animated, and the scroll clamps down with it in one movement rather than jumping to the new
 * bottom.
 *
 * The *surface* does not move with it. A GPU-backed surface keeps presenting its last drawn frame while its
 * bounds change, mapped into the new bounds by whatever scaling the platform applies, and that produced two
 * separate faults. Expanding in one step stretched a single strip-height frame down the whole opened card: a
 * visible smear. Collapsing is worse and subtler — every frame was drawn for a slightly taller box than the
 * one it is shown in, so the contents sit off by however much the box shrank during that frame. The offset
 * peaks in the middle of the ease and is zero at both ends, so the coins dart one way and come back, most
 * visibly on the top row, whose real position barely changes between the two arrangements. Anchoring the
 * mapping fixes the expand and not the collapse: a smooth resize is a fresh mismatch every frame whatever
 * the anchor.
 *
 * So the surface is sized once to the roomiest arrangement these coins can be put into, picked-out band
 * included, and never more than one screen — worked out here, before anything asks for it, because the grid
 * layout is pure and cheap — and the card clips it as it opens and closes. It changes only when the holdings
 * or the width do, which is never during a flight. On a grid taller than the screen it slides down the field
 * with the scroll instead, so a frame costs the same however many coins the wallet holds.
 *
 * The renderer lays out against the width handed to it rather than against its own surface, so its grid and
 * the one measured here are the same grid rather than two that agree to within a rounding.
 *
 * Worth knowing when testing: these artifacts were far more visible on an emulator than on hardware, which
 * draws the replacement frame sooner. "I cannot see it on my phone" is not evidence that it is gone.
 */
@Composable
internal fun CoinageCoins(
    modifier: Modifier = Modifier,
    coins: ImmutableList<CoinageScene.Coin>,
    isExpanded: Boolean,
    /** The picked coin, already known to be among [coins]. Ignored while collapsed. */
    focusedCoin: String?,
    stripHeight: Dp = CoinageStripLayout.Options().height.dp,
    onMetrics: (CoinageCoinsMetrics) -> Unit,
    onCoinTapped: (String?) -> Unit,
    onFlickedDown: () -> Unit,
    detail: @Composable (String) -> Unit
) {
    val budget = gridBudget()
    val density = LocalDensity.current
    val windowHeight = LocalWindowInfo.current.containerSize.height
    val bottomBars = maxOf(
        LocalAppNavigationBarInsets.current.getBottom(density),
        WindowInsets.navigationBars.getBottom(density)
    )
    var placement by remember { mutableStateOf(Placement()) }
    var view by remember { mutableStateOf<CoinageCoinsView?>(null) }
    val tapped = rememberUpdatedState(onCoinTapped)

    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .onGloballyPositioned { coordinates ->
                val top = coordinates.positionInWindow().y
                val visible = coordinates.boundsInWindow()

                placement = Placement(
                    visibleTop = maxOf(visible.top - top, 0f),
                    clearBottom = windowHeight - bottomBars - top,
                    screenBottom = windowHeight - top
                )
            },
        contentAlignment = Alignment.TopStart
    ) {
        val width = maxWidth.value
        val gridHeight = remember(coins, width, budget) {
            CoinageGridLayout.layout(
                coins.map { CoinageGridLayout.Item(it.id, it.exponent, it.partition) },
                areaWidth = width,
                areaHeight = budget
            ).height
        }

        val picked = focusedCoin.takeIf { isExpanded }
        val focus = picked?.let { id ->
            with(density) {
                CoinageArrangement.Focus(
                    id = id,
                    band = maxOf(CoinageTraceHeight.value, CoinageArrangement.FOCUS_BAND_MINIMUM),
                    visibleTop = placement.visibleTop.toDp().value,
                    visibleBottom = placement.clearBottom.toDp().value
                )
            }
        }
        val bandTop = focus?.let { CoinageArrangement.bandTop(it, gridHeight) }

        // The most the field is ever asked to be for these coins, which is what keeps the surface from being
        // resized under a flight: picking a coin or scrolling never changes it.
        val roomiest = maxOf(gridHeight + CoinageArrangement.FOCUS_BAND_MINIMUM, stripHeight.value)
        val wanted = when {
            !isExpanded -> stripHeight.value
            focus != null && bandTop != null -> CoinageArrangement.fieldHeight(focus, gridHeight, bandTop)
            else -> gridHeight
        }.coerceAtLeast(stripHeight.value)
        val room = remember { Animatable(wanted) }

        LaunchedEffect(wanted) {
            if (wanted >= room.value) {
                room.snapTo(wanted)
            } else {
                room.animateTo(wanted, tween(CollapseMillis, easing = FastOutSlowInEasing))
            }
        }

        val screenPixels = windowHeight.toFloat()
        val surfacePixels = with(density) { minOf(roomiest.dp.toPx(), screenPixels) }.roundToInt()
        val roomPixels = with(density) { room.value.dp.toPx() }
        // The surface covers the screen with any spare height split above and below the viewport, so a frame
        // drawn a scroll step late still has the coins it needs.
        val span = placement.screenBottom - placement.visibleTop
        val slack = maxOf(surfacePixels - span, 0f) / 2
        val windowPixels = (placement.visibleTop - slack)
            .coerceIn(0f, maxOf(roomPixels - surfacePixels, 0f))
            .roundToInt()
        val window = with(density) { windowPixels.toDp().value }

        AndroidView(
            modifier = Modifier
                .fillMaxWidth()
                .height(room.value.dp)
                .pointerInput(Unit) {
                    detectTapGestures { offset ->
                        view?.tap(offset.x / density.density, offset.y / density.density) { tapped.value(it) }
                    }
                }
                .then(if (isExpanded) Modifier else Modifier.flickDown(onFlickedDown)),
            factory = { context -> CoinageCoinsView(context).also { view = it } },
            update = { coinsView ->
                coinsView.onMetrics = onMetrics
                coinsView.update(
                    coins = coins,
                    isExpanded = isExpanded,
                    areaWidth = width,
                    stripHeight = stripHeight.value,
                    gridBudget = budget,
                    surfaceHeight = surfacePixels,
                    focus = focus,
                    window = window
                )
            }
        )

        // The last band position is kept through the fade-out, so the detail leaves from where it was.
        val anchor = remember { BandAnchor() }

        if (bandTop != null) anchor.top = bandTop

        AnimatedContent(
            modifier = Modifier
                .offset(y = anchor.top.dp)
                .padding(start = (CoinageArrangement.FOCUS_DIAMETER + CoinageArrangement.FOCUS_INSET * 2).dp)
                .fillMaxWidth(),
            targetState = picked,
            transitionSpec = { fadeIn() togetherWith fadeOut() }
        ) { shown ->
            if (shown != null) {
                Box(modifier = Modifier.fillMaxWidth().height(focus?.band?.dp ?: CoinageTraceHeight)) {
                    detail(shown)
                }
            }
        }
    }
}

/** Not state: only read in the composition that wrote it. */
private class BandAnchor(var top: Float = 0f)

/** Where the field is on screen, in window pixels relative to its own top. */
private data class Placement(
    val visibleTop: Float = 0f,
    val clearBottom: Float = Float.MAX_VALUE,
    val screenBottom: Float = Float.MAX_VALUE
)

/**
 * A flick down on the strip, which unfurls it. Watches without consuming anything, so the page's own scroll
 * never waits on it or fails because of it: a slow drag still scrolls, and a flick is noticed on the way past.
 */
private fun Modifier.flickDown(onFlick: () -> Unit) = pointerInput(onFlick) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
        val tracker = VelocityTracker()
        var last = down.position

        tracker.addPosition(down.uptimeMillis, down.position)

        while (true) {
            val change = awaitPointerEvent(PointerEventPass.Initial).changes.firstOrNull { it.id == down.id } ?: break

            tracker.addPosition(change.uptimeMillis, change.position)
            last = change.position

            if (!change.pressed) break
        }

        val travel = last - down.position
        val speed = tracker.calculateVelocity().y

        if (travel.y > FlickDistance.toPx() &&
            abs(travel.y) > abs(travel.x) * 2 &&
            speed > FlickSpeed.toPx()
        ) {
            onFlick()
        }
    }
}

/** How tall the detail beside a picked coin needs to be: a headline and two lines, with an inset each way. */
internal val CoinageTraceHeight = (CoinageArrangement.FOCUS_INSET * 2 + 30f + 18f * 2).dp

private val FlickDistance = 24.dp
private val FlickSpeed = 600.dp

/**
 * The height the grid is packed against, which is what makes the coins shrink at all.
 *
 * It has to be a real height or nothing ever fails to fit: the grid would keep the largest coins whatever the
 * count, and five hundred holdings would lay out about five thousand points tall — past what a drawable can
 * be, which is what turned the five hundred coin grid into a blur on iOS.
 *
 * A screenful, because the grid is meant to be taken in at a glance. Overflowing it costs a scroll rather
 * than a redraw: the coins are already at their smallest by then.
 */
@Composable
private fun gridBudget(): Float {
    val screen = LocalConfiguration.current.screenHeightDp.toFloat()

    return maxOf(screen * GRID_SCREENFUL, MINIMUM_GRID_BUDGET)
}

/** What a budget is before a composition has measured the screen. */
internal const val DEFAULT_GRID_BUDGET = 480f

private const val GRID_SCREENFUL = 0.62f
private const val MINIMUM_GRID_BUDGET = 240f
private const val CollapseMillis = 350
