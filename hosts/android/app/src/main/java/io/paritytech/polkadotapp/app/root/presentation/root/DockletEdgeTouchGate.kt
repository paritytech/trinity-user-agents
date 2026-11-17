package io.paritytech.polkadotapp.app.root.presentation.root

import kotlin.math.abs

// A Back swipe's down reaches the app before the system takes the gesture and cancels the stream; a steeper move never becomes Back.
class DockletEdgeTouchGate(private val touchSlop: Float) {
    private var held: DockletInput.Touch? = null

    fun onDown(touch: DockletInput.Touch, onBackGestureEdge: Boolean): DockletInput.Touch? {
        held = touch.takeIf { onBackGestureEdge }
        return touch.takeUnless { onBackGestureEdge }
    }

    fun onMove(x: Float, y: Float): DockletInput.Touch? {
        val down = held ?: return null
        val dy = abs(y - down.y)
        if (dy <= touchSlop || dy <= abs(x - down.x)) return null
        held = null
        return down
    }

    fun onUp(): DockletInput.Touch? = held.also { held = null }

    fun onCancel() {
        held = null
    }
}
