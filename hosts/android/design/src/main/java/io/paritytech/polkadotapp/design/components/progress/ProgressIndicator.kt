package io.paritytech.polkadotapp.design.components.progress

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun NovaCircularProgressIndicator(
    modifier: Modifier = Modifier,
    color: Color = PolkadotTheme.colors.fg.primary,
    strokeWidth: Dp = 4.dp,
    trackColor: Color = PolkadotTheme.colors.fg.tertiary,
    strokeCap: StrokeCap = StrokeCap.Round
) {
    CircularProgressIndicator(
        modifier = modifier,
        color = color,
        strokeWidth = strokeWidth,
        trackColor = trackColor,
        strokeCap = strokeCap
    )
}

@Composable
fun NovaCircularProgressIndicator(
    modifier: Modifier = Modifier,
    progress: () -> Float,
    color: Color = PolkadotTheme.colors.fg.primary,
    strokeWidth: Dp = 4.dp,
    trackColor: Color = Color.Transparent,
    strokeCap: StrokeCap = StrokeCap.Square
) {
    CircularProgressIndicator(
        modifier = modifier,
        progress = progress,
        color = color,
        trackColor = trackColor,
        strokeCap = strokeCap,
        strokeWidth = strokeWidth
    )
}

@Composable
fun NovaLinearProgressIndicator(
    progress: Float,
    modifier: Modifier = Modifier,
    color: Color = PolkadotTheme.colors.fg.primary,
    trackColor: Color = PolkadotTheme.colors.fg.primary.copy(alpha = 0.3f),
    shape: Shape = PolkadotTheme.shapes.full,
    thickness: Dp = 4.dp
) {
    Box(
        modifier = modifier
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(thickness)
                .background(trackColor, shape)
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(fraction = progress)
                    .height(thickness)
                    .background(color, shape)
            )
        }
    }
}

@Composable
fun LoadingScreenState(
    modifier: Modifier = Modifier
        .fillMaxSize()
        .systemBarsPadding()
) {
    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center
    ) {
        NovaCircularProgressIndicator()
    }
}
