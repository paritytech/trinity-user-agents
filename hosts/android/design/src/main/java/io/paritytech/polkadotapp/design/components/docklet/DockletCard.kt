package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.AnimatedVisibilityScope
import androidx.compose.animation.EnterExitState
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationVector1D
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.blurEffect
import kotlin.math.min

private val HandleWidth = 36.dp
private val HandleHeight = 5.dp
private val HandleRadius = 3.dp
private val HandleTop = 6.dp
private const val MaxFontScale = 1.3f
private val ContentsInMillis =
    (DockletMotion.ContentsInDelay + DockletMotion.ContentsInDuration).inWholeMilliseconds.toInt()
private val ContentsOutMillis = DockletMotion.ContentsOutDuration.inWholeMilliseconds.toInt()

internal class DockletDragExclusion {
    internal var bounds: Rect? = null
}

internal val LocalDockletDragExclusion = staticCompositionLocalOf<DockletDragExclusion?> { null }

internal fun Modifier.dockletDragExclusion(exclusion: DockletDragExclusion?): Modifier =
    if (exclusion == null) this else onGloballyPositioned { exclusion.bounds = it.boundsInRoot() }

@Composable
fun DockletCard(
    state: DockletCardState,
    frames: () -> DockletCardFrames,
    backdrop: DockletBackdrop?,
    closeLabel: String,
    lobeIconColor: @Composable () -> Color,
    onOpenRequest: (DockletStep) -> Unit,
    onCloseRequest: () -> Unit,
    onKeyboardRequest: () -> Unit,
    lobeAnchor: @Composable BoxScope.() -> Unit,
    content: @Composable () -> Unit,
) {
    val exclusion = remember { DockletDragExclusion() }
    val pressScale = remember { Animatable(1f) }
    val bodyGestures = rememberDockletCardGestures(
        state = state,
        frames = frames,
        pressScale = pressScale,
        exclusion = exclusion,
        lobe = false,
        onOpenRequest = onOpenRequest,
        onCloseRequest = onCloseRequest,
        onKeyboardRequest = onKeyboardRequest,
    )
    val lobeGestures = rememberDockletCardGestures(
        state = state,
        frames = frames,
        pressScale = pressScale,
        exclusion = exclusion,
        lobe = true,
        onOpenRequest = onOpenRequest,
        onCloseRequest = onCloseRequest,
        onKeyboardRequest = onKeyboardRequest,
    )
    val restBody = bodyGestures.semantics {
        role = Role.Button
        onClick { onOpenRequest(DockletStep.Step1); true }
    }
    DockletGlassTheme {
        if (state.motionMode == DockletMotionMode.CrossFade) {
            val restOwnsGestures = !state.isOpen || state.swipingUp
            val cardShown by remember(state) { derivedStateOf { state.isOpen || state.crossFadeProgress > 0f } }
            CardLayer(
                frame = { frames().rest },
                alpha = { 1f - state.crossFadeProgress },
                morphProgress = { 0f },
                pressScale = pressScale,
                backdrop = backdrop,
            ) {
                CardBody(modifier = if (restOwnsGestures) restBody else Modifier, coversLobe = { true }) {}
                PillLobe(
                    modifier = if (restOwnsGestures) lobeGestures else Modifier,
                    frame = { frames().rest },
                    progress = { 0f },
                    open = false,
                    closeLabel = closeLabel,
                    iconColor = lobeIconColor,
                    onClose = onCloseRequest,
                    anchor = lobeAnchor,
                )
            }
            if (cardShown) {
                CardLayer(
                    frame = { state.frame(frames()) },
                    alpha = { state.crossFadeProgress },
                    morphProgress = { 1f },
                    pressScale = pressScale,
                    backdrop = backdrop,
                ) {
                    CardBody(modifier = if (restOwnsGestures) Modifier else bodyGestures, coversLobe = { false }) {
                        CardContents(state, frames, exclusion, onCloseRequest, content)
                    }
                    PillLobe(
                        modifier = if (restOwnsGestures) Modifier else lobeGestures,
                        frame = { state.frame(frames()) },
                        progress = { 1f },
                        open = state.isOpen,
                        closeLabel = closeLabel,
                        iconColor = lobeIconColor,
                        onClose = onCloseRequest,
                        anchor = {},
                    )
                }
            }
        } else {
            CardLayer(
                frame = { state.frame(frames()) },
                alpha = { 1f },
                morphProgress = { state.clampedProgress },
                pressScale = pressScale,
                backdrop = backdrop,
            ) {
                CardBody(modifier = if (state.isOpen) bodyGestures else restBody, coversLobe = { !state.isOpen }) {
                    CardContents(state, frames, exclusion, onCloseRequest, content)
                }
                PillLobe(
                    modifier = lobeGestures,
                    frame = { state.frame(frames()) },
                    progress = { state.clampedProgress },
                    open = state.isOpen,
                    closeLabel = closeLabel,
                    iconColor = lobeIconColor,
                    onClose = onCloseRequest,
                    anchor = lobeAnchor,
                )
            }
        }
    }
}

@Composable
fun AnimatedVisibilityScope.dockletContentsTransition(): Modifier {
    val alpha by transition.animateFloat(
        transitionSpec = {
            if (targetState == EnterExitState.Visible) {
                tween(ContentsInMillis, easing = DockletMotion.ContentsInEasing)
            } else {
                tween(ContentsOutMillis, easing = DockletMotion.ContentsOutEasing)
            }
        },
        label = "DockletContentsAlpha",
    ) { state -> if (state == EnterExitState.Visible) 1f else 0f }

    return Modifier.graphicsLayer {
        val frame = DockletMotion.contentsFrame(alpha)
        this.alpha = frame.alpha
        translationY = frame.rise.toPx()
        renderEffect = blurEffect(frame.blurSigma, density)
    }
}

@Composable
private fun CardLayer(
    frame: () -> DockletFrame,
    alpha: () -> Float,
    morphProgress: () -> Float,
    pressScale: Animatable<Float, AnimationVector1D>,
    backdrop: DockletBackdrop?,
    content: @Composable BoxScope.() -> Unit,
) {
    val shape = remember(morphProgress) { DockletCardShape { 1f - morphProgress() } }
    val palette = LocalDockletPalette.current
    Box(
        modifier = Modifier
            .placeAt(frame)
            .graphicsLayer {
                this.alpha = alpha()
                scaleX = pressScale.value
                scaleY = pressScale.value
            }
            .dockletBackdrop(backdrop, shape) { DockletMaterials.PillBlurSigma }
            .drawWithCache {
                val path = cardPath(size, density, 1f - morphProgress())
                onDrawBehind { drawDockletGlass(path, palette) }
            },
        content = content,
    )
}

@Composable
private fun CardBody(modifier: Modifier, coversLobe: () -> Boolean, content: @Composable () -> Unit) {
    Box(
        modifier = Modifier.layout { measurable, constraints ->
            val lobe = if (coversLobe()) 0 else DockletGeometry.LobeDepth.roundToPx()
            val height = (constraints.maxHeight - lobe).coerceAtLeast(0)
            val placeable = measurable.measure(Constraints.fixed(constraints.maxWidth, height))
            layout(placeable.width, placeable.height) { placeable.place(0, 0) }
        },
        propagateMinConstraints = true,
    ) {
        Box(modifier = modifier) {
            content()
        }
    }
}

@Composable
private fun CardContents(
    state: DockletCardState,
    frames: () -> DockletCardFrames,
    exclusion: DockletDragExclusion,
    onCloseRequest: () -> Unit,
    content: @Composable () -> Unit,
) {
    val density = LocalDensity.current
    val cappedDensity = remember(density) { Density(density.density, min(density.fontScale, MaxFontScale)) }

    AnimatedVisibility(
        visible = state.isOpen,
        enter = EnterTransition.None,
        exit = ExitTransition.None,
    ) {
        Layout(
            modifier = Modifier
                .clipToBounds()
                .then(dockletContentsTransition())
                .graphicsLayer {
                    if (state.swipeDriven) alpha = DockletMotion.swipeContentsAlpha(state.progress)
                }
                .nestedScroll(rememberDockletListDrag(state, frames, onCloseRequest)),
            content = {
                Box {
                    CompositionLocalProvider(
                        LocalDensity provides cappedDensity,
                        LocalDockletDragExclusion provides exclusion,
                    ) {
                        content()
                    }
                    PolkadotSurface(
                        modifier = Modifier
                            .align(Alignment.TopCenter)
                            .padding(top = HandleTop)
                            .size(HandleWidth, HandleHeight),
                        shape = RoundedCornerShape(HandleRadius),
                        color = LocalDockletPalette.current.handle,
                    ) {}
                }
            },
        ) { measurables, constraints ->
            val open = state.openFrame(frames())
            val placeable = measurables.single().measure(
                Constraints.fixed(open.width.roundToPx(), open.height.roundToPx())
            )
            layout(constraints.maxWidth, constraints.maxHeight) {
                placeable.place((constraints.maxWidth - placeable.width) / 2, 0)
            }
        }
    }
}

private fun Modifier.placeAt(frame: () -> DockletFrame): Modifier = this
    .offset {
        val current = frame()
        IntOffset(current.left.roundToPx(), current.top.roundToPx())
    }
    .layout { measurable, _ ->
        val current = frame()
        val width = current.width.roundToPx()
        val height = (current.height + DockletGeometry.LobeDepth).roundToPx()
        val placeable = measurable.measure(Constraints.fixed(width, height))
        layout(width, height) { placeable.place(0, 0) }
    }

@Preview(widthDp = 392, heightDp = 240)
@Composable
private fun DockletCardRestPreview() {
    PolkadotTheme {
        Box(modifier = Modifier.fillMaxSize()) {
            val dock = DockletGeometry.dockFrame(392.dp, 240.dp, 24.dp, threeButtonNavigation = false)
            val pill = DockletGeometry.pillFrame(dock)
            val frames = DockletCardFrames(
                rest = DockletFrame(pill.left, pill.top, pill.width, pill.height - DockletGeometry.LobeDepth),
                step1 = DockletGeometry.step1CardFrame(dock),
                step2 = DockletGeometry.step2CardFrame(dock, keyboardTop = 240.dp, statusInset = 24.dp),
            )
            DockletCard(
                state = rememberDockletCardState(),
                frames = { frames },
                backdrop = null,
                closeLabel = "",
                lobeIconColor = { PolkadotTheme.colors.fg.secondary },
                onOpenRequest = {},
                onCloseRequest = {},
                onKeyboardRequest = {},
                lobeAnchor = {},
            ) {}
        }
    }
}
