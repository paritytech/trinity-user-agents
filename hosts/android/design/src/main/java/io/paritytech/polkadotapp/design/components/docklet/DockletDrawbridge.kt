package io.paritytech.polkadotapp.design.components.docklet

import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.pow
import kotlin.math.sin

@Immutable
data class DockletPose(val angle: Float, val dx: Float, val dy: Float) {
    val angleDegrees: Float get() = angle * DegreesPerRadian

    fun plus(other: DockletPose, factor: Float) =
        DockletPose(angle + factor * other.angle, dx + factor * other.dx, dy + factor * other.dy)

    fun minus(other: DockletPose) = plus(other, -1f)

    companion object {
        val Zero = DockletPose(0f, 0f, 0f)
        private const val DegreesPerRadian = (180.0 / PI).toFloat()
    }
}

// y-down coordinates: a positive angle turns the dock clockwise and swings its left end up.
@Immutable
class DockletDrawbridge(dock: DockletFrame, screenWidth: Dp, screenHeight: Dp) {
    val dockLeft = dock.left.value
    private val dockTop = dock.top.value
    private val dockRight = dock.right.value
    private val dockBottom = dock.bottom.value
    private val dockWidth = dock.width.value
    private val width = screenWidth.value
    val height = screenHeight.value

    val pivot = DockletPoint(dockRight + PivotGap, dockTop + dock.height.value / 2)
    private val slideOut = width - dockRight + ExtraSlide

    val glowTop: Float = max(GlowTopClearance, pivot.y - (pivot.x - dockLeft))
    private val glowLength: Float = min(EdgeZoneLength, max(0f, height - GlowBottomClearance - glowTop))

    val glowFrame = DockletFrame((width - GlowWidth).dp, glowTop.dp, GlowWidth.dp, glowLength.dp)
    val grabZone = DockletFrame((width - GrabZoneWidth).dp, glowTop.dp, GrabZoneWidth.dp, glowLength.dp)

    fun plainPose(progress: Float) = DockletPose(QuarterTurn * progress, slideOut * max(0f, progress).pow(SlideExponent), 0f)

    fun map(point: DockletPoint, pose: DockletPose): DockletPoint {
        val x = point.x - pivot.x
        val y = point.y - pivot.y
        val cos = cos(pose.angle)
        val sin = sin(pose.angle)
        return DockletPoint(x * cos - y * sin + pivot.x + pose.dx, x * sin + y * cos + pivot.y + pose.dy)
    }

    fun unmap(point: DockletPoint, pose: DockletPose): DockletPoint {
        val x = point.x - pivot.x - pose.dx
        val y = point.y - pivot.y - pose.dy
        val cos = cos(pose.angle)
        val sin = sin(pose.angle)
        return DockletPoint(x * cos + y * sin + pivot.x, -x * sin + y * cos + pivot.y)
    }

    fun lowestY(pose: DockletPose): Float {
        val sin = sin(pose.angle)
        val cos = cos(pose.angle)
        val left = dockLeft - pivot.x
        val right = dockRight - pivot.x
        val top = dockTop - pivot.y
        val bottom = dockBottom - pivot.y
        val lowestOffset = maxOf(
            maxOf(left * sin + top * cos, right * sin + top * cos),
            maxOf(left * sin + bottom * cos, right * sin + bottom * cos),
        )
        return lowestOffset + pivot.y + pose.dy
    }

    fun reaches(point: DockletPoint): Boolean =
        point.x in dockLeft - GrabSlop..dockRight + GrabSlop && point.y in dockTop - GrabSlop..dockBottom + GrabSlop

    fun hideTravel(touchX: Float): Float = min(HideTravelShare * dockWidth, max(MinEdgeTravel, EdgeTravelMultiple * (width - touchX)))

    fun backBand(): ClosedFloatingPointRange<Float> = glowTop - BackBandSlop..dockBottom + BackBandSlop

    fun aimedProgress(point: DockletPoint, progress: Float): Float =
        rawAim(point, rawAim(point, progress))

    private fun rawAim(point: DockletPoint, progress: Float): Float {
        val slidPivotX = pivot.x + plainPose(progress).dx
        return atan2(max(0f, pivot.y - point.y), max(1f, slidPivotX - point.x)) / QuarterTurn
    }

    companion object {
        const val QuarterTurn = (PI / 2).toFloat()
        const val GlowWidth = 24f
        private const val PivotGap = 14f
        private const val ExtraSlide = 60f
        private const val GrabZoneWidth = 47f
        private const val EdgeZoneLength = 200f
        private const val GlowTopClearance = 56f
        private const val GlowBottomClearance = 8f
        private const val GrabSlop = 16f
        private const val BackBandSlop = 24f
        private const val SlideExponent = 3
        private const val HideTravelShare = 0.75f
        private const val EdgeTravelMultiple = 1.2f
        private const val MinEdgeTravel = 60f
        private const val OvershootCap = 0.25f
        private const val OvershootRate = 2.2f
        private const val PeekDepth = 0.14f
        private const val QuietFadeScale = 1.12f
        private const val QuietFadePower = 1.15f

        fun rubberBand(distance: Float, limit: Float): Float = limit * (1f - 1f / (1f + distance / limit))

        private fun overshoot(beyond: Float): Float = OvershootCap * (1f - 1f / (1f + OvershootRate * beyond))

        fun progressFromTravel(raw: Float): Float = when {
            raw < 0f -> -overshoot(-raw)
            raw > 1f -> 1f + overshoot(raw - 1f)
            else -> raw
        }

        fun smoothstep(value: Float): Float {
            val clamped = value.coerceIn(0f, 1f)
            return clamped * clamped * (3f - 2f * clamped)
        }

        fun peekDip(fraction: Float): Float = PeekDepth * (1f - cos(2f * PI.toFloat() * fraction.coerceIn(0f, 1f))) / 2f

        fun quietFadeAlpha(progress: Float): Float =
            (1f - QuietFadeScale * progress.coerceIn(0f, 1f).pow(QuietFadePower)).coerceIn(0f, 1f)
    }
}

class DockletHold(
    private val drawbridge: DockletDrawbridge,
    touch: DockletPoint,
    private val startProgress: Float,
    travel: Float,
    screenPose: DockletPose,
) {
    private val grabbed: DockletPoint? = drawbridge.unmap(touch, screenPose).takeIf(drawbridge::reaches)
    private val zeroX = touch.x - startProgress * travel
    private val startAngle = screenPose.angle
    private var finger by mutableStateOf(touch)
    private var progress by mutableFloatStateOf(startProgress)

    fun track(location: DockletPoint, current: Float) {
        finger = location
        progress = current
    }

    fun pose(): DockletPose {
        val grabbedPoint = grabbed ?: return drawbridge.plainPose(progress)
        val floor = drawbridge.height + DockletDrawbridge.rubberBand(max(0f, finger.y - drawbridge.height), FloorLimit)
        val pose = heldPose(grabbedPoint, fittingTurnShare(grabbedPoint, floor))
        val sink = drawbridge.lowestY(pose) - floor
        return if (sink > 0f) pose.copy(dy = pose.dy - sink) else pose
    }

    private fun fittingTurnShare(grabbedPoint: DockletPoint, allowedBottom: Float): Float {
        if (drawbridge.lowestY(heldPose(grabbedPoint, TurnShare)) <= allowedBottom) return TurnShare

        var fitting = 0f
        var overflowing = TurnShare
        repeat(FitSteps) {
            val middle = (fitting + overflowing) / 2f
            if (drawbridge.lowestY(heldPose(grabbedPoint, middle)) <= allowedBottom) fitting = middle else overflowing = middle
        }
        return fitting
    }

    private fun heldPose(grabbedPoint: DockletPoint, turnShare: Float): DockletPose {
        val angle = startAngle + turnShare * DockletDrawbridge.QuarterTurn * (progress - startProgress)
        val target = targetPoint(grabbedPoint)
        val turned = drawbridge.map(grabbedPoint, DockletPose(angle, 0f, 0f))
        return DockletPose(angle, target.x - turned.x, target.y - turned.y)
    }

    private fun targetPoint(grabbedPoint: DockletPoint): DockletPoint {
        val glowTop = drawbridge.glowTop
        val fingerX = if (finger.x >= zeroX) finger.x else zeroX - DockletDrawbridge.rubberBand(zeroX - finger.x, WrongWayLimit)
        val fingerY = if (finger.y < glowTop) glowTop - DockletDrawbridge.rubberBand(glowTop - finger.y, CeilingLimit) else finger.y
        return DockletPoint(fingerX + lead(grabbedPoint), fingerY)
    }

    private fun lead(grabbedPoint: DockletPoint): Float {
        val cue = LeadDistance * (DockletDrawbridge.smoothstep(progress) - DockletDrawbridge.smoothstep(startProgress))
        val cap = grabbedPoint.x - drawbridge.dockLeft - LeadInset
        return max(0f, min(cue, cap))
    }

    private companion object {
        const val TurnShare = 0.5f
        const val FitSteps = 12
        const val LeadDistance = 56f
        const val LeadInset = 14f
        const val WrongWayLimit = 40f
        const val CeilingLimit = 40f
        const val FloorLimit = 36f
    }
}

class DockletAim(
    private val drawbridge: DockletDrawbridge,
    private val touch: DockletPoint,
    private val startProgress: Float,
    screenPose: DockletPose,
) {
    private val startAim = drawbridge.aimedProgress(touch, startProgress)
    private val startResidual = screenPose.minus(drawbridge.plainPose(startProgress))
    private var finger by mutableStateOf(touch)

    var progress by mutableFloatStateOf(startProgress)
        private set

    fun track(location: DockletPoint) {
        finger = location
        val aimed = drawbridge.aimedProgress(location, progress)
        progress = (aimed + (startProgress - startAim) * blend()).coerceIn(0f, 1f)
    }

    fun pose(): DockletPose = drawbridge.plainPose(progress).plus(startResidual, blend())

    private fun blend(): Float = max(0f, 1f - hypot(finger.x - touch.x, finger.y - touch.y) / BlendDistance)

    private companion object {
        const val BlendDistance = 80f
    }
}
