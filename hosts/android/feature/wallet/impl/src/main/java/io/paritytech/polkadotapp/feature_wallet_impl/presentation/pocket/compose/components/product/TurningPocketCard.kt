package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawOutline
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.launch
import uniffi.truapi.PocketCardSide
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The opened card with both its faces drawn, the one turned away waiting behind, so a turn shows it
 * at once. Only a card with a back turns, and a card [leaving] turns back to the front the list shows.
 */
@Composable
internal fun TurningPocketCard(
    modifier: Modifier = Modifier,
    card: PocketCardUiModel.ProductCard,
    front: ProductFaceBindings,
    back: ProductFaceBindings,
    leaving: Boolean,
    turn: CardTurnState = rememberCardTurnState(),
) {
    val backFace by back.face.collectAsStateWithLifecycle()
    val turnable = backFace != null && !leaving
    LaunchedEffect(backFace == null) { if (backFace == null) turn.snapHome() }
    LaunchedEffect(leaving) { if (leaving) turn.turnHome() }

    val scope = rememberCoroutineScope()
    val turnOver = stringResource(RCommon.string.pocket_card_turn_over)
    var widthPx by remember { mutableFloatStateOf(1f) }

    Box(
        modifier = modifier
            .onSizeChanged { widthPx = it.width.toFloat() }
            .draggable(
                orientation = Orientation.Horizontal,
                enabled = turnable,
                state = rememberDraggableState { delta -> turn.drag(delta, widthPx) },
                onDragStarted = { turn.beginDrag() },
                onDragStopped = { velocity -> turn.release(velocity, widthPx) },
            )
            .semantics {
                if (turnable) {
                    customActions = listOf(
                        CustomAccessibilityAction(turnOver) {
                            scope.launch { turn.turnOver() }
                            true
                        }
                    )
                }
            },
    ) {
        val thickness = with(LocalDensity.current) { CARD_THICKNESS.toPx() }
        CardEdge(turn, thickness)
        CardFace(card, front, PocketCardSide.FRONT, turn, depth = -thickness / 2)
        CardFace(card, back, PocketCardSide.BACK, turn, depth = thickness / 2)
    }
}

/**
 * The card's thickness: strips along its side nearer the viewer, stacked between the faces. Turning about the
 * vertical axis never shows the top, bottom or far edges.
 */
@Composable
private fun BoxScope.CardEdge(turn: CardTurnState, thickness: Float) {
    val shape = PolkadotTheme.shapes.large
    repeat(EDGE_SLICES) { slice ->
        val depth = thickness * (slice / (EDGE_SLICES - 1f) - 0.5f)
        Box(
            Modifier
                .matchParentSize()
                .turned(turn, PocketCardSide.FRONT, depth)
                .drawBehind {
                    val outline = shape.createOutline(size, layoutDirection, this)
                    // The edge catches more light the more of it faces the viewer.
                    val color = lerp(EDGE_SHADED, EDGE_LIT, turn.sideOn)
                    val strip = EDGE_STRIP.toPx()
                    val left = if (turn.leftSideNearer) 0f else size.width - strip
                    clipRect(left = left, right = left + strip) { drawOutline(outline, color) }
                }
        )
    }
}

/** The face up lies on top, where its surface takes every press; the face turned away is not read aloud. */
@Composable
private fun CardFace(
    card: PocketCardUiModel.ProductCard,
    bindings: ProductFaceBindings,
    side: PocketCardSide,
    turn: CardTurnState,
    depth: Float,
) {
    val shown by remember(turn, side) { derivedStateOf { turn.shownSide == side } }

    ProductPocketCard(
        modifier = Modifier
            .zIndex(if (shown) 1f else 0f)
            .turned(turn, side, depth)
            .alpha(if (shown) 1f else 0f)
            .then(if (shown) Modifier else Modifier.clearAndSetSemantics {}),
        card = card,
        bindings = bindings,
        onOpen = null,
        onRemoveRequested = null,
    )
}

/** Draws a layer of the card [depth] behind its middle, as [side] is seen at the current turn. */
private fun Modifier.turned(turn: CardTurnState, side: PocketCardSide, depth: Float) = graphicsLayer {
    rotationY = turn.layerRotation(side)
    translationX = turn.depthOffset(depth)
    scaleX = turn.scale
    scaleY = turn.scale
    cameraDistance = CAMERA_DISTANCE_DP * density
}

private const val CAMERA_DISTANCE_DP = 12f
private val CARD_THICKNESS = 4.dp
private const val EDGE_SLICES = 6
private val EDGE_STRIP = 24.dp
private val EDGE_SHADED = Color(0xFF16171A)
private val EDGE_LIT = Color(0xFF4A4C53)
