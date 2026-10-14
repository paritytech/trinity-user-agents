package io.paritytech.polkadotapp.design.components.progress

import androidx.annotation.FloatRange
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

@Composable
fun SegmentedArcIndicator(
    modifier: Modifier = Modifier,
    currentSegments: Int,
    totalSegments: Int,
    activeColor: Color = Color.White,
    inactiveColor: Color = Color.DarkGray,
    strokeWidth: Dp = 10.dp,
    @FloatRange(from = 0.0, to = 1.0) gapPercentage: Float = 0.15f
) {
    SubcomposeLayout(modifier = modifier) { constraints ->
        val indicatorWidthPx = if (constraints.hasBoundedWidth || constraints.hasFixedWidth) {
            constraints.maxWidth
        } else {
            if (constraints.minWidth > 0 && constraints.minWidth != Constraints.Infinity) {
                constraints.minWidth
            } else {
                0
            }
        }

        val indicatorRequiredHeightPx = (indicatorWidthPx / 2f) + (strokeWidth.toPx() / 2f)
        val indicatorRequiredHeightInt = indicatorRequiredHeightPx.roundToInt().coerceAtLeast(0)

        val placeable = subcompose(Unit) {
            SegmentedArcIndicatorInternal(
                currentSegments = currentSegments,
                totalSegments = totalSegments,
                activeColor = activeColor,
                inactiveColor = inactiveColor,
                strokeWidth = strokeWidth,
                gapPercentage = gapPercentage
            )
        }.map { measurable ->
            measurable.measure(
                Constraints.fixed(
                    width = indicatorWidthPx,
                    height = indicatorRequiredHeightInt
                )
            )
        }.first()

        layout(indicatorWidthPx, indicatorRequiredHeightInt) {
            placeable.placeRelative(0, 0)
        }
    }
}

@Composable
private fun SegmentedArcIndicatorInternal(
    currentSegments: Int,
    totalSegments: Int,
    activeColor: Color,
    inactiveColor: Color,
    strokeWidth: Dp,
    gapPercentage: Float
) {
    val fixedStartAngle = 180f
    val fixedSweepAngleRange = 180f

    val numActiveSegments = currentSegments.coerceIn(0, totalSegments)

    val actualSegmentArcAngle: Float
    val actualGapArcAngle: Float

    if (totalSegments == 1) {
        actualSegmentArcAngle = fixedSweepAngleRange
        actualGapArcAngle = 0f
    } else {
        if (gapPercentage == 1.0f) {
            actualSegmentArcAngle = 0f
            actualGapArcAngle = fixedSweepAngleRange / (totalSegments - 1)
        } else {
            val ratioGapToSegment = gapPercentage / (1f - gapPercentage)
            val denominator = totalSegments + (totalSegments - 1) * ratioGapToSegment
            if (denominator <= 0.0001f) {
                actualSegmentArcAngle = 0f
                actualGapArcAngle = fixedSweepAngleRange / (totalSegments - 1)
            } else {
                actualSegmentArcAngle = fixedSweepAngleRange / denominator
                actualGapArcAngle = actualSegmentArcAngle * ratioGapToSegment
            }
        }
    }

    Canvas(modifier = Modifier) {
        val strokeStyle = Stroke(width = strokeWidth.toPx(), cap = StrokeCap.Round)

        val outerVisualDiameter = size.width

        val centerlineDiameter = outerVisualDiameter - strokeWidth.toPx()

        val arcBoundingBoxTopLeft = Offset(
            x = (outerVisualDiameter - centerlineDiameter) / 2f,
            y = (outerVisualDiameter - centerlineDiameter) / 2f
        )

        val arcBoundingBoxSize = Size(centerlineDiameter, centerlineDiameter)

        var currentDrawAngle = fixedStartAngle
        for (i in 0 until totalSegments) {
            val color = if (i < numActiveSegments) activeColor else inactiveColor
            if (actualSegmentArcAngle > 0.0001f) {
                drawArc(
                    color = color,
                    startAngle = currentDrawAngle,
                    sweepAngle = actualSegmentArcAngle,
                    useCenter = false,
                    topLeft = arcBoundingBoxTopLeft,
                    size = arcBoundingBoxSize,
                    style = strokeStyle
                )
            }
            currentDrawAngle += actualSegmentArcAngle + actualGapArcAngle
        }
    }
}
