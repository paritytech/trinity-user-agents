package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose

import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.BoundsTransform
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.SharedTransitionScope
import androidx.compose.animation.core.CubicBezierEasing
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
import androidx.compose.ui.zIndex
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel

internal const val POCKET_ANIM_DURATION_MS = 500

private const val POCKET_PUSHED_SCALE = 0.9f

private val PocketOvershootEasing = CubicBezierEasing(0.34f, 1.2f, 0.64f, 1f)

val LocalSharedTransitionScope = compositionLocalOf<SharedTransitionScope?> { null }
val LocalNavAnimatedVisibilityScope = compositionLocalOf<AnimatedVisibilityScope?> { null }

internal val PocketBoundsTransform = BoundsTransform { _, _ -> tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing) }

internal fun pocketSlideIn(initialOffsetY: (fullHeight: Int) -> Int): EnterTransition =
    slideInVertically(tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing), initialOffsetY)

internal fun pocketSlideOut(targetOffsetY: (fullHeight: Int) -> Int): ExitTransition =
    slideOutVertically(tween(POCKET_ANIM_DURATION_MS, easing = PocketOvershootEasing), targetOffsetY)

internal fun pocketFadeIn(): EnterTransition = fadeIn(tween(POCKET_ANIM_DURATION_MS))

internal fun pocketFadeOut(): ExitTransition = fadeOut(tween(POCKET_ANIM_DURATION_MS))

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
                boundsTransform = PocketBoundsTransform
            )
        }
    }
}

@Composable
fun Modifier.pocketBalanceSharedElement(cardId: String): Modifier {
    val sharedScope = LocalSharedTransitionScope.current
    val visibilityScope = LocalNavAnimatedVisibilityScope.current
    return if (sharedScope == null || visibilityScope == null) {
        this
    } else {
        with(sharedScope) {
            this@pocketBalanceSharedElement.sharedElement(
                sharedContentState = rememberSharedContentState("pocket_balance_$cardId"),
                animatedVisibilityScope = visibilityScope,
                boundsTransform = PocketBoundsTransform
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
