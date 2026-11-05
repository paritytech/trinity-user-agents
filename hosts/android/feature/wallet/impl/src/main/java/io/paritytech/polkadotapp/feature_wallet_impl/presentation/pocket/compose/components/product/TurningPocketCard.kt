package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
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
            .testTag(PocketTestTags.TURNING_CARD)
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
                            scope.launch { turn.turnTo(turn.shownSide.other()) }
                            true
                        }
                    )
                }
            },
    ) {
        CardFace(card, front, PocketCardSide.FRONT, turn)
        CardFace(card, back, PocketCardSide.BACK, turn)
    }
}

/** The face up lies on top, where its surface takes every press; the face turned away is not read aloud. */
@Composable
private fun CardFace(card: PocketCardUiModel.ProductCard, bindings: ProductFaceBindings, side: PocketCardSide, turn: CardTurnState) {
    val shown by remember(turn, side) { derivedStateOf { turn.shownSide == side } }

    ProductPocketCard(
        modifier = Modifier
            .zIndex(if (shown) 1f else 0f)
            .graphicsLayer {
                rotationY = turn.layerRotation(side)
                cameraDistance = CAMERA_DISTANCE_DP * density
                alpha = if (shown) 1f else 0f
            }
            .then(if (shown) Modifier else Modifier.clearAndSetSemantics {}),
        card = card,
        bindings = bindings,
        onOpen = null,
        onRemoveRequested = null,
    )
}

private fun PocketCardSide.other() = if (this == PocketCardSide.FRONT) PocketCardSide.BACK else PocketCardSide.FRONT

private const val CAMERA_DISTANCE_DP = 12f
