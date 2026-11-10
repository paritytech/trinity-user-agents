package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.runtime.Immutable
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

@Immutable
data class DockletPoint(val x: Float, val y: Float)

@Immutable
data class CubicSegment(val control1: DockletPoint, val control2: DockletPoint, val end: DockletPoint)

@Immutable
data class DockletFrame(val left: Dp, val top: Dp, val width: Dp, val height: Dp) {
    val right: Dp get() = left + width
    val bottom: Dp get() = top + height
    val centerX: Dp get() = left + width / 2
}

@Immutable
data class DockletSpan(val start: Dp, val end: Dp) {
    val width: Dp get() = end - start
}

@Immutable
data class DockletCutout(val start: DockletPoint, val segments: List<CubicSegment>)

@Immutable
data class DockletCardOutline(
    val width: Float,
    val height: Float,
    val cornerRadius: Float,
    val lobeStart: DockletPoint,
    val lobe: List<CubicSegment>,
)

@Immutable
data class DockletOutline(val start: DockletPoint, val segments: List<CubicSegment>)

object DockletGeometry {
    val DockHeight = 62.dp
    val PillDiameter = 64.dp
    val PillProud = 10.dp
    val CutoutHalfWidth = 37.dp
    val SlotWidth = 80.dp
    val ItemRowPadding = 4.dp
    val ItemRowHeight = 54.dp
    val CardCornerRadius = 32.dp
    val LobeDepth = 31.dp
    val LobeHalfWidth = 44.dp
    val Step1BodyHeight = 204.dp
    val Step1Rise = 2.dp
    val CardBodyBelowDockTop = 22.dp
    val KeyboardGap = 4.dp
    val Step2TopGap = 12.dp
    val ListRowHeight = 64.dp
    const val MaxListRows = 8

    private val SideMargin = 10.dp
    private val WideScreenThreshold = 460.dp
    private val MaxDockWidth = 440.dp
    private val GestureInsetTrim = 7.dp
    private val MinBottomGap = 12.dp
    private val ThreeButtonGap = 8.dp

    val FieldHeight = 48.dp
    val Step2ContentTop = 18.dp
    val Step2ContentBottom = 14.dp
    private val Step2HeaderLine = 16.dp
    private val Step2PartGap = 4.dp
    private val Step2BodyChrome =
        Step2ContentTop + Step2HeaderLine + Step2PartGap * 2 + FieldHeight + Step2ContentBottom
    private val MaxStep2BodyHeight = Step2BodyChrome + ListRowHeight * MaxListRows

    fun dockFrame(screenWidth: Dp, screenHeight: Dp, navigationInset: Dp, threeButtonNavigation: Boolean): DockletFrame {
        val width = if (screenWidth > WideScreenThreshold) MaxDockWidth else screenWidth - SideMargin * 2
        val bottomGap = if (threeButtonNavigation) {
            navigationInset + ThreeButtonGap
        } else {
            max((navigationInset - GestureInsetTrim).value, MinBottomGap.value).dp
        }

        return DockletFrame(
            left = (screenWidth - width) / 2,
            top = screenHeight - bottomGap - DockHeight,
            width = width,
            height = DockHeight,
        )
    }

    fun pillFrame(dock: DockletFrame): DockletFrame = DockletFrame(
        left = dock.centerX - PillDiameter / 2,
        top = dock.top - PillProud,
        width = PillDiameter,
        height = PillDiameter,
    )

    fun cutout(centerX: Float): DockletCutout {
        fun p(dx: Float, y: Float) = DockletPoint(centerX + dx, y)

        return DockletCutout(
            start = p(-37f, 0f),
            segments = listOf(
                CubicSegment(p(-33f, 0f), p(-31f, 5f), p(-32f, 9f)),
                CubicSegment(p(-33f, 13f), p(-34f, 17f), p(-34f, 22f)),
                CubicSegment(p(-34f, 41f), p(-19f, 56f), p(0f, 56f)),
                CubicSegment(p(19f, 56f), p(34f, 41f), p(34f, 22f)),
                CubicSegment(p(34f, 17f), p(33f, 13f), p(32f, 9f)),
                CubicSegment(p(31f, 5f), p(33f, 0f), p(37f, 0f)),
            ),
        )
    }

    fun slotCentres(dockWidth: Dp): List<Dp> {
        val centre = dockWidth / 2
        val half = SlotWidth / 2
        return listOf(
            ItemRowPadding + half,
            centre - CutoutHalfWidth - half,
            centre + CutoutHalfWidth + half,
            dockWidth - ItemRowPadding - half,
        )
    }

    fun touchSpans(dockWidth: Dp): List<DockletSpan> {
        val centres = slotCentres(dockWidth)
        val centre = dockWidth / 2
        val leftMid = (centres[0] + centres[1]) / 2
        val rightMid = (centres[2] + centres[3]) / 2
        return listOf(
            DockletSpan(ItemRowPadding, leftMid),
            DockletSpan(leftMid, centre - CutoutHalfWidth),
            DockletSpan(centre + CutoutHalfWidth, rightMid),
            DockletSpan(rightMid, dockWidth - ItemRowPadding),
        )
    }

    fun step1CardFrame(dock: DockletFrame): DockletFrame {
        val bodyBottom = dock.top - Step1Rise + CardBodyBelowDockTop
        return DockletFrame(dock.left, bodyBottom - Step1BodyHeight, dock.width, Step1BodyHeight)
    }

    fun step1DockTop(dock: DockletFrame): Dp = dock.top - Step1Rise

    internal fun step2DockBottom(dock: DockletFrame, keyboardTop: Dp): Dp =
        min(dock.bottom.value, (keyboardTop - KeyboardGap).value).dp

    fun keyboardDock(dock: DockletFrame, keyboardTop: Dp): DockletFrame =
        dock.copy(top = step2DockBottom(dock, keyboardTop) - DockHeight)

    fun step2CardFrame(dock: DockletFrame, keyboardTop: Dp, statusInset: Dp): DockletFrame {
        val dockTop = keyboardDock(dock, keyboardTop).top
        val bodyBottom = dockTop + CardBodyBelowDockTop
        val top = max((statusInset + Step2TopGap).value, (bodyBottom - MaxStep2BodyHeight).value).dp
        return DockletFrame(dock.left, top, dock.width, (bodyBottom - top).coerceAtLeast(0.dp))
    }

    fun cardOutline(width: Float, height: Float): DockletCardOutline {
        val radius = minOf(CardCornerRadius.value, width / 2, height / 2)
        val k = min(1f, width / LobeHalfWidth.value / 2)
        val centre = width / 2

        fun p(dx: Int, dy: Float) = DockletPoint(centre + (dx * k).roundToInt(), height + dy)

        return DockletCardOutline(
            width = width,
            height = height,
            cornerRadius = radius,
            lobeStart = p(44, 0f),
            lobe = listOf(
                CubicSegment(p(37, 0f), p(32, 5f), p(30, 11f)),
                CubicSegment(p(26, 23f), p(13, 31f), p(0, 31f)),
                CubicSegment(p(-13, 31f), p(-26, 23f), p(-30, 11f)),
                CubicSegment(p(-32, 5f), p(-37, 0f), p(-44, 0f)),
            ),
        )
    }

    // Card and Ø64 Pill traced with the same 13 cubics, so the outline blends between them without a jump.
    fun cardOutlineCurves(width: Float, height: Float, pillShare: Float): DockletOutline {
        val card = cardCurves(width, height)
        if (pillShare <= 0f) return card
        val pill = pillCurves(width, height)
        val share = pillShare.coerceAtMost(1f)
        return DockletOutline(
            start = card.start.lerp(pill.start, share),
            segments = card.segments.zip(pill.segments) { a, b ->
                CubicSegment(a.control1.lerp(b.control1, share), a.control2.lerp(b.control2, share), a.end.lerp(b.end, share))
            },
        )
    }

    private fun cardCurves(width: Float, height: Float): DockletOutline {
        val outline = cardOutline(width, height)
        val r = outline.cornerRadius
        val handle = r * ARC_KAPPA
        val lobeEnd = outline.lobe.last().end
        val segments = listOf(
            line(DockletPoint(r, 0f), DockletPoint(width - r, 0f)),
            CubicSegment(DockletPoint(width - r + handle, 0f), DockletPoint(width, r - handle), DockletPoint(width, r)),
            line(DockletPoint(width, r), DockletPoint(width, height - r)),
            CubicSegment(
                DockletPoint(width, height - r + handle),
                DockletPoint(width - r + handle, height),
                DockletPoint(width - r, height),
            ),
            line(DockletPoint(width - r, height), outline.lobeStart),
        ) + outline.lobe + listOf(
            line(lobeEnd, DockletPoint(r, height)),
            CubicSegment(DockletPoint(r - handle, height), DockletPoint(0f, height - r + handle), DockletPoint(0f, height - r)),
            line(DockletPoint(0f, height - r), DockletPoint(0f, r)),
            CubicSegment(DockletPoint(0f, r - handle), DockletPoint(r - handle, 0f), DockletPoint(r, 0f)),
        )
        return DockletOutline(DockletPoint(r, 0f), segments)
    }

    private fun pillCurves(width: Float, height: Float): DockletOutline {
        val radius = PillDiameter.value / 2
        val cx = width / 2
        val cy = height + LobeDepth.value - radius
        val quarter = radius * ARC_KAPPA
        val eighth = radius * EIGHTH_ARC_HANDLE
        val diagonal = radius * DIAGONAL
        val top = DockletPoint(cx, cy - radius)
        val right = DockletPoint(cx + radius, cy)
        val bottom = DockletPoint(cx, cy + radius)
        val left = DockletPoint(cx - radius, cy)
        val bottomRight = DockletPoint(cx + diagonal, cy + diagonal)
        val bottomLeft = DockletPoint(cx - diagonal, cy + diagonal)
        val eighthStep = eighth * DIAGONAL
        val segments = listOf(
            point(top),
            CubicSegment(DockletPoint(cx + quarter, cy - radius), DockletPoint(cx + radius, cy - quarter), right),
            point(right),
            point(right),
            point(right),
            CubicSegment(
                DockletPoint(right.x, right.y + eighth),
                DockletPoint(bottomRight.x + eighthStep, bottomRight.y - eighthStep),
                bottomRight,
            ),
            CubicSegment(
                DockletPoint(bottomRight.x - eighthStep, bottomRight.y + eighthStep),
                DockletPoint(bottom.x + eighth, bottom.y),
                bottom,
            ),
            CubicSegment(
                DockletPoint(bottom.x - eighth, bottom.y),
                DockletPoint(bottomLeft.x + eighthStep, bottomLeft.y + eighthStep),
                bottomLeft,
            ),
            CubicSegment(
                DockletPoint(bottomLeft.x - eighthStep, bottomLeft.y - eighthStep),
                DockletPoint(left.x, left.y + eighth),
                left,
            ),
            point(left),
            point(left),
            point(left),
            CubicSegment(DockletPoint(cx - radius, cy - quarter), DockletPoint(cx - quarter, cy - radius), top),
        )
        return DockletOutline(top, segments)
    }

    private fun line(from: DockletPoint, to: DockletPoint) =
        CubicSegment(from.lerp(to, 1f / 3), from.lerp(to, 2f / 3), to)

    private fun point(at: DockletPoint) = CubicSegment(at, at, at)

    private fun DockletPoint.lerp(other: DockletPoint, fraction: Float) =
        DockletPoint(x + (other.x - x) * fraction, y + (other.y - y) * fraction)

    // Cubic handle lengths for a quarter (4/3·tan(π/8)) and an eighth (4/3·tan(π/16)) of a circle.
    private const val ARC_KAPPA = 0.5522848f
    private const val EIGHTH_ARC_HANDLE = 0.2652031f
    private const val DIAGONAL = 0.70710677f
}
