package io.paritytech.polkadotapp.common.utils

import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventTimeoutCancellationException
import androidx.compose.ui.input.pointer.PointerInputScope

suspend fun PointerInputScope.longPressIgnoreChildren(
    onLongPress: () -> Unit,
) {
    awaitEachGesture {
        val down = awaitFirstDown(pass = PointerEventPass.Initial)

        var longPressTriggered = false
        try {
            withTimeout(viewConfiguration.longPressTimeoutMillis) {
                waitForUpOrCancellation(pass = PointerEventPass.Initial)
            }
        } catch (e: PointerEventTimeoutCancellationException) {
            longPressTriggered = true
            onLongPress()
        }

        if (longPressTriggered) {
            down.consume()

            while (true) {
                val event = awaitPointerEvent(pass = PointerEventPass.Initial)
                event.changes.forEach { it.consume() }

                if (event.changes.all { !it.pressed }) break
            }
        }
    }
}
