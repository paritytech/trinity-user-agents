package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import kotlinx.collections.immutable.ImmutableList

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
 * So the surface is sized once to the roomiest arrangement these coins can be put into — worked out here,
 * before anything asks for it, because the grid layout is pure and cheap — and the card clips it as it opens
 * and closes. It changes only when the holdings or the width do, which is never during a flight. There is no
 * mapping left to get wrong in either direction, and coins below the strip line stay visible until the clip
 * passes over them: the card closing over them rather than coins vanishing on the tap. The cost is memory,
 * not frames — the render loop is paused whenever the springs are at rest.
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
    stripHeight: Dp = CoinageStripLayout.Options().height.dp,
    onMetrics: (CoinageCoinsMetrics) -> Unit
) {
    val budget = gridBudget()

    BoxWithConstraints(
        modifier = modifier.fillMaxWidth(),
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

        val surface = maxOf(gridHeight, stripHeight.value)
        val wanted = if (isExpanded) surface else stripHeight.value
        val room = remember { Animatable(wanted) }

        LaunchedEffect(wanted) {
            if (wanted >= room.value) {
                room.snapTo(wanted)
            } else {
                room.animateTo(wanted, tween(CollapseMillis, easing = FastOutSlowInEasing))
            }
        }

        // The card animates this view's height; the texture inside keeps the surface's, and the view clips
        // it. Only an ordinary view is ever resized.
        val surfacePixels = with(LocalDensity.current) { surface.dp.roundToPx() }

        AndroidView(
            modifier = Modifier.fillMaxWidth().height(room.value.dp),
            factory = { context -> CoinageCoinsView(context) },
            update = { view ->
                view.onMetrics = onMetrics
                view.update(
                    coins = coins,
                    isExpanded = isExpanded,
                    areaWidth = width,
                    stripHeight = stripHeight.value,
                    gridBudget = budget,
                    surfaceHeight = surfacePixels
                )
            }
        )
    }
}

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
