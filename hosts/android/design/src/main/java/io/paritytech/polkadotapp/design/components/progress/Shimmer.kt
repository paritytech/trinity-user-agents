package io.paritytech.polkadotapp.design.components.progress

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun Shimmer(
    modifier: Modifier,
    shape: Shape = PolkadotTheme.shapes.large
) {
    val shimmerOffset = rememberShimmerOffset(durationMillis = 2000)

    Box(
        modifier = modifier.background(
            brush = shimmerBrush(
                edge = Color(0x1FFFFFFF),
                leftMid = Color(0x05FFFFFF),
                center = Color(0x1FFFFFFF),
                rightMid = Color(0x4DFFFFFF),
                start = Offset(shimmerOffset - 1000f, shimmerOffset - 1000f),
                end = Offset(shimmerOffset, shimmerOffset)
            ),
            shape = shape
        )
    )
}

@Composable
fun rememberShimmerBrush(): Brush {
    val shimmerOffset = rememberShimmerOffset(durationMillis = 1000)
    return shimmerBrush(
        edge = Color(0xFF000000),
        leftMid = Color(0x73000000),
        center = Color(0x4D000000),
        rightMid = Color(0x73000000),
        start = Offset(shimmerOffset, 0f),
        end = Offset(shimmerOffset + 1000f, 0f)
    )
}

@Composable
private fun rememberShimmerOffset(durationMillis: Int): Float {
    val offset by rememberInfiniteTransition(label = "shimmer").animateFloat(
        initialValue = -1000f,
        targetValue = 2000f,
        animationSpec = infiniteRepeatable(
            animation = tween(durationMillis, easing = LinearEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "shimmerOffset"
    )
    return offset
}

private fun shimmerBrush(
    edge: Color,
    leftMid: Color,
    center: Color,
    rightMid: Color,
    start: Offset,
    end: Offset
): Brush = Brush.linearGradient(
    colorStops = arrayOf(
        0.0f to edge,
        0.35f to leftMid,
        0.5f to center,
        0.65f to rightMid,
        1.0f to edge,
    ),
    start = start,
    end = end
)

@Preview(backgroundColor = 0xff000000, showBackground = true)
@Composable
private fun ShimmerPreview() {
    PolkadotTheme {
        Shimmer(modifier = Modifier.width(200.dp).height(46.dp))
    }
}
