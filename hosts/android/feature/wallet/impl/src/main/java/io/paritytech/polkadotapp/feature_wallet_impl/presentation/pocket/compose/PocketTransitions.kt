package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose

import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.BoundsTransform
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SharedTransitionScope
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.Transition
import androidx.compose.animation.core.VisibilityThreshold
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.zIndex
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketScreenState
import kotlin.time.Duration.Companion.milliseconds

internal const val POCKET_ANIM_DURATION_MS = 500

// SwiftUI's default .easeInOut, which the iOS card fades its corner balance with.

private const val POCKET_PUSHED_SCALE = 0.9f

private val PocketOvershootEasing = CubicBezierEasing(0.34f, 1.2f, 0.64f, 1f)

internal const val COLLECTIBLES_PRESSED_SCALE = 0.98f
internal const val COLLECTIBLES_PRESS_DURATION_MS = 80
internal const val COLLECTIBLES_COLOUR_DURATION_MS = 170
internal const val COLLECTIBLES_LIFT_DURATION_MS = 400

internal val CollectiblesLiftRequestDelay = 65.milliseconds
internal const val COLLECTIBLES_LIFT_TILT_DEGREES = -4f
internal const val COLLECTIBLES_POCKET_DIM_DURATION_MS = 180

private const val COLLECTIBLES_LIFT_STIFFNESS = 260f

private val CollectiblesLiftBoundsTransform = BoundsTransform { _, _ ->
    spring(Spring.DampingRatioNoBouncy, COLLECTIBLES_LIFT_STIFFNESS, Rect.VisibilityThreshold)
}

val LocalSharedTransitionScope = compositionLocalOf<SharedTransitionScope?> { null }
val LocalNavAnimatedVisibilityScope = compositionLocalOf<AnimatedVisibilityScope?> { null }

@Composable
internal fun isPocketScreenLeaving(): Boolean =
    LocalNavAnimatedVisibilityScope.current?.transition?.targetState == EnterExitState.PostExit
val LocalPocketScreenTransition = compositionLocalOf<Transition<PocketScreenState>> {
    noLocalProvidedFor("LocalPocketScreenTransition")
}

internal val PocketBoundsTransform = BoundsTransform { _, _ -> tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing) }

internal fun pocketSlideIn(initialOffsetY: (fullHeight: Int) -> Int): EnterTransition =
    slideInVertically(tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing), initialOffsetY)

internal fun pocketSlideOut(targetOffsetY: (fullHeight: Int) -> Int): ExitTransition =
    slideOutVertically(tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing), targetOffsetY)

internal fun pocketFadeIn(durationMillis: Int = POCKET_ANIM_DURATION_MS): EnterTransition = fadeIn(tween(durationMillis))

internal fun pocketFadeOut(durationMillis: Int = POCKET_ANIM_DURATION_MS): ExitTransition = fadeOut(tween(durationMillis))

internal fun pocketScaleIn(): EnterTransition =
    scaleIn(tween(POCKET_ANIM_DURATION_MS), initialScale = POCKET_PUSHED_SCALE)

internal fun pocketScaleOut(): ExitTransition =
    scaleOut(tween(POCKET_ANIM_DURATION_MS), targetScale = POCKET_PUSHED_SCALE)

@Composable
fun Modifier.pocketCardSharedElement(index: Int): Modifier {
    val sharedScope = LocalSharedTransitionScope.current
    val visibilityScope = LocalNavAnimatedVisibilityScope.current
    return if (sharedScope == null || visibilityScope == null) {
        this
    } else {
        with(sharedScope) {
            this@pocketCardSharedElement.sharedElement(
                sharedContentState = rememberSharedContentState("pocket_card_$index"),
                animatedVisibilityScope = visibilityScope,
                boundsTransform = PocketBoundsTransform,
                zIndexInOverlay = index.toFloat()
            )
        }
    }
}

@Composable
fun Modifier.pocketCollectiblesImageSharedElement(): Modifier {
    val sharedScope = LocalSharedTransitionScope.current
    val visibilityScope = LocalNavAnimatedVisibilityScope.current
    return if (sharedScope == null || visibilityScope == null) {
        this
    } else {
        with(sharedScope) {
            this@pocketCollectiblesImageSharedElement.sharedElement(
                sharedContentState = rememberSharedContentState("pocket_collectibles_image"),
                animatedVisibilityScope = visibilityScope,
                boundsTransform = CollectiblesLiftBoundsTransform
            )
        }
    }
}

@Composable
fun Modifier.pocketContentSlide(): Modifier {
    val visibilityScope = LocalNavAnimatedVisibilityScope.current
    return if (visibilityScope == null) {
        this
    } else {
        with(visibilityScope) {
            this@pocketContentSlide.animateEnterExit(
                enter = pocketSlideIn { it },
                exit = pocketSlideOut { it }
            )
        }
    }
}

@Composable
fun Modifier.pocketListCardSharedElement(
    card: PocketCardUiModel,
    index: Int,
    anchorCard: PocketCardUiModel?,
    anchorIndex: Int
): Modifier {
    val sharedScope = LocalSharedTransitionScope.current
    val visibilityScope = LocalNavAnimatedVisibilityScope.current
    val sharedElementModifier = pocketCardSharedElement(index)
    val isAnchorOrNoAnchor = card.id == anchorCard?.id || anchorIndex < 0
    val withSlide = if (sharedScope == null || visibilityScope == null || isAnchorOrNoAnchor) {
        sharedElementModifier
    } else {
        with(sharedScope) {
            with(visibilityScope) {
                val isBelowAnchor = index > anchorIndex
                // Every card here is drawn in the shared overlay, which the screen's own fade does
                // not reach, so each one carries its own: without it a card above the picked one
                // stays fully opaque and sits under it the whole way up.
                val enter = if (isBelowAnchor) {
                    pocketSlideIn { fullHeight -> fullHeight * 4 } + pocketFadeIn()
                } else {
                    pocketScaleIn() + pocketFadeIn()
                }
                val exit = if (isBelowAnchor) {
                    pocketSlideOut { fullHeight -> fullHeight * 4 } + pocketFadeOut()
                } else {
                    pocketScaleOut() + pocketFadeOut()
                }
                sharedElementModifier
                    .renderInSharedTransitionScopeOverlay(zIndexInOverlay = index.toFloat())
                    .animateEnterExit(enter = enter, exit = exit)
            }
        }
    }
    return withSlide.zIndex(index.toFloat())
}
