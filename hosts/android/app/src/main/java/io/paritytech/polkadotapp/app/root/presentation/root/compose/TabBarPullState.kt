package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

private const val UNSET = Float.MAX_VALUE

/**
 * UI state of the right-edge pull-out bar: how far it is pulled in ([offset]) and which panel above it, if
 * any, is expanded. Owns the cancellable settle animation, so the bar can be re-grabbed mid-flight instead
 * of being locked until it finishes. Held via `remember` in the overlay; the gesture loop feeds it raw
 * pointer deltas and nothing else touches its fields.
 */
@Stable
class TabBarPullState(
    private val scope: CoroutineScope,
    private val nubPx: Float,
    private val flingVelocityPx: Float
) {
    private var barWidthPx by mutableFloatStateOf(0f)

    // The live pull, in px from the right edge. UNSET means "not dragged yet" → rest at maxOffset.
    private var offsetX by mutableFloatStateOf(UNSET)
    private var settleJob: Job? = null

    // Offset captured on press, so a tap-while-open can undo the press-peek and return there.
    private var pressAnchor = 0f

    private var expandedPanel by mutableStateOf(ExpandedPanel.None)

    val appsExpanded: Boolean get() = expandedPanel == ExpandedPanel.Apps

    val networkStatusExpanded: Boolean get() = expandedPanel == ExpandedPanel.NetworkStatus

    val scanExpanded: Boolean get() = expandedPanel == ExpandedPanel.Scan

    val panelExpanded: Boolean get() = expandedPanel != ExpandedPanel.None

    /** Fully-collapsed pull — only the nub showing. */
    val maxOffset: Float get() = (barWidthPx - nubPx).coerceAtLeast(0f)

    /** Current pull, resolving the "not dragged yet" sentinel to the collapsed rest position. */
    val offset: Float get() = if (offsetX == UNSET) maxOffset else offsetX

    val isOpen: Boolean get() = maxOffset > 0f && offset < maxOffset

    val isFullyOpen: Boolean get() = maxOffset > 0f && offset == 0f

    /** Visible width of the bar from the right edge — the nub when collapsed, the full bar when open. */
    val visibleWidthPx: Float get() = (barWidthPx - offset).coerceIn(0f, barWidthPx)

    fun setBarWidth(px: Float) { barWidthPx = px }

    fun collapsePanels() { expandedPanel = ExpandedPanel.None }

    fun toggleApps() = toggle(ExpandedPanel.Apps)

    fun toggleNetworkStatus() = toggle(ExpandedPanel.NetworkStatus)

    fun toggleScan() = toggle(ExpandedPanel.Scan)

    fun openScan() { expandedPanel = ExpandedPanel.Scan }

    /** Snap to the screen's default posture: open when [forceShown] (Main), collapsed otherwise. */
    fun snapToDefault(forceShown: Boolean) {
        if (maxOffset <= 0f) return
        collapsePanels()
        val target = if (forceShown) 0f else maxOffset
        if (offsetX == UNSET) offsetX = target else settleTo(target)
    }

    fun collapse() {
        collapsePanels()
        settleTo(maxOffset)
    }

    // --- Gesture ops, driven by the overlay's pointer loop ---

    /** Pop the bar out from under the finger on press; returns whether it was collapsed (nub-only). */
    fun peek(peekPx: Float): Boolean {
        settleJob?.cancel()
        val current = offset
        pressAnchor = current
        offsetX = (current - peekPx).coerceIn(0f, maxOffset)
        return current >= maxOffset - 1f
    }

    fun dragBy(inwardPx: Float) {
        offsetX = (offset + inwardPx).coerceIn(0f, maxOffset)
    }

    /** Release after a drag: a fling past the velocity threshold wins, else open past the halfway point. */
    fun settleAfterDrag(inwardVelocityPx: Float) {
        val target = when {
            inwardVelocityPx > flingVelocityPx -> maxOffset
            inwardVelocityPx < -flingVelocityPx -> 0f
            offset < maxOffset / 2f -> 0f
            else -> maxOffset
        }
        if (target == maxOffset) collapsePanels()
        settleTo(target, inwardVelocityPx)
    }

    /** Tap on the collapsed nub — open the bar. */
    fun openFromNub() = settleTo(0f)

    /** Tap while already open — just undo the press-peek. */
    fun undoPeek() = settleTo(pressAnchor)

    private fun toggle(panel: ExpandedPanel) {
        expandedPanel = if (expandedPanel == panel) ExpandedPanel.None else panel
    }

    private fun settleTo(target: Float, initialVelocity: Float = 0f) {
        settleJob?.cancel()
        settleJob = scope.launch {
            animate(
                initialValue = if (offsetX == UNSET) target else offsetX,
                targetValue = target,
                initialVelocity = initialVelocity,
                animationSpec = spring(dampingRatio = 0.85f, stiffness = Spring.StiffnessMedium),
            ) { value, _ -> offsetX = value }
        }
    }
}

private enum class ExpandedPanel { None, Apps, NetworkStatus, Scan }
