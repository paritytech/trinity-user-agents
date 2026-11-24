package io.paritytech.polkadotapp.feature_products_api.model

import uniffi.truapi.GestureEvent
import uniffi.truapi.GesturePhase
import uniffi.truapi.encodeGestureEvent

/** The SCALE `GestureEvent` a gesture action carries, encoded by the core's own codec. */
fun JsUiEvent.Type.Gesture.toGesturePayload(): ByteArray = encodeGestureEvent(
    GestureEvent(
        phase = phase.toCore(),
        x = x,
        y = y,
        fractionPermille = fractionPermille.coerceIn(0, GESTURE_PERMILLE).toUShort(),
    ),
)

private const val GESTURE_PERMILLE = 1000

private fun JsGesturePhase.toCore(): GesturePhase = when (this) {
    JsGesturePhase.TAP -> GesturePhase.TAP
    JsGesturePhase.STARTED -> GesturePhase.STARTED
    JsGesturePhase.MOVED -> GesturePhase.MOVED
    JsGesturePhase.ENDED -> GesturePhase.ENDED
    JsGesturePhase.CANCELLED -> GesturePhase.CANCELLED
    JsGesturePhase.COMPLETED -> GesturePhase.COMPLETED
}
