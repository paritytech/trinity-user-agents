package io.paritytech.polkadotapp.design.components.tooltip

import androidx.compose.ui.unit.*
import androidx.compose.ui.window.PopupPositionProvider

internal class TooltipPositionProvider(
    private val screenPaddingPx: Int,
    private val tooltipAlignment: TooltipAlignment,
    private val onLayout: (parentBounds: IntRect, tooltipBounds: IntRect, arrowCenterOffset: IntOffset) -> Unit,
) : PopupPositionProvider {
    override fun calculatePosition(
        anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize
    ): IntOffset {
        val popupX = if (windowSize.width - popupContentSize.width <= screenPaddingPx) {
            0
        } else {
            when (tooltipAlignment) {
                TooltipAlignment.Top, TooltipAlignment.Bottom -> anchorBounds.center.x - popupContentSize.center.x
                TooltipAlignment.Start -> anchorBounds.centerLeft.x - popupContentSize.width
                TooltipAlignment.End -> anchorBounds.centerRight.x
            }.coerceAtLeast(0)
        }

        val popupY = when (tooltipAlignment) {
            TooltipAlignment.Top -> anchorBounds.top - popupContentSize.height
            TooltipAlignment.Bottom -> anchorBounds.bottom
            TooltipAlignment.Start -> anchorBounds.centerLeft.y - popupContentSize.center.y
            TooltipAlignment.End -> anchorBounds.centerRight.y - popupContentSize.center.y
        }.coerceAtLeast(0)

        val tooltipBounds = IntRect(
            left = popupX,
            top = popupY,
            right = popupX + popupContentSize.width,
            bottom = popupY + popupContentSize.height,
        )

        val arrowCenterOffset = getArrowCenterOffset(
            popupX = popupX,
            popupY = popupY,
            anchorBounds = anchorBounds,
            windowSize = windowSize,
            popupContentSize = popupContentSize,
        )

        onLayout(anchorBounds, tooltipBounds, arrowCenterOffset)

        return IntOffset(popupX, popupY)
    }

    private fun getArrowCenterOffset(
        popupX: Int,
        popupY: Int,
        anchorBounds: IntRect,
        windowSize: IntSize,
        popupContentSize: IntSize,
    ): IntOffset {
        val actualPopupX = popupX.coerceAtMost(windowSize.width - popupContentSize.width)
        val actualPopupY = popupY.coerceAtMost(windowSize.height - popupContentSize.height)

        return when (tooltipAlignment) {
            TooltipAlignment.Top, TooltipAlignment.Bottom -> IntOffset(
                x = when {
                    // left side
                    actualPopupX == 0 -> {
                        anchorBounds.center.x
                    }
                    // right side
                    (windowSize.width - actualPopupX - popupContentSize.width).coerceAtLeast(0) == 0 -> {
                        anchorBounds.bottomRight.x - anchorBounds.width / 2 - actualPopupX
                    }
                    // in the middle of the screen
                    else -> {
                        popupContentSize.width / 2
                    }
                },
                y = 0
            )
            TooltipAlignment.Start, TooltipAlignment.End -> IntOffset(
                x = 0,
                y = when {
                    // top side
                    actualPopupY == 0 -> {
                        anchorBounds.center.y
                    }
                    // bottom side
                    (windowSize.height - actualPopupY - popupContentSize.height).coerceAtLeast(0) == 0 -> {
                        anchorBounds.bottomRight.y - anchorBounds.height / 2 - actualPopupY
                    }
                    // in the middle of the screen
                    else -> {
                        popupContentSize.height / 2
                    }
                }
            )
        }
    }
}
