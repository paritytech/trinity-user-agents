package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private const val SKELETON_SWEEP_LOOP_MS = 1350
private const val SKELETON_SWEEP_BAND_WIDTH = 0.5f
private const val SKELETON_SWEEP_ALPHA = 0.12f
private const val SKELETON_DOT_ALPHA = 0.25f
private const val SKELETON_DOT_COLUMNS = 12

private val SkeletonTitleSize = DpSize(136.dp, 16.dp)
private val SkeletonSubtitleSize = DpSize(180.dp, 10.dp)
private val SkeletonButtonSize = 40.dp
private val SkeletonFirstCardHeight = 260.dp
private val SkeletonFirstLabelWidth = 80.dp
private val SkeletonSecondLabelWidth = 116.dp
private val SkeletonLabelHeight = 14.dp
private val SkeletonDotPitch = 24.dp
private val SkeletonDotRadius = 2.5.dp

@Composable
fun CollectiblesSkeleton(modifier: Modifier, sweeping: Boolean) {
    Column(
        modifier = modifier
            .skeletonSweep(sweeping)
            .padding(PolkadotTheme.spacings.mediumIncreased)
    ) {
        VerticalSpacer { small }
        Box(modifier = Modifier.fillMaxWidth()) {
            Column(
                modifier = Modifier.align(Alignment.Center),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                SkeletonBlock(modifier = Modifier.size(SkeletonTitleSize), shape = PolkadotTheme.shapes.small)
                VerticalSpacer { small }
                SkeletonBlock(modifier = Modifier.size(SkeletonSubtitleSize), shape = PolkadotTheme.shapes.full)
            }
            SkeletonBlock(
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .size(SkeletonButtonSize),
                shape = PolkadotTheme.shapes.full
            )
        }
        VerticalSpacer { small }
        SkeletonCard(
            modifier = Modifier
                .fillMaxWidth()
                .height(SkeletonFirstCardHeight),
            labelWidth = SkeletonFirstLabelWidth
        )
        VerticalSpacer { large }
        SkeletonCard(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            labelWidth = SkeletonSecondLabelWidth
        )
    }
}

@Composable
private fun SkeletonCard(modifier: Modifier, labelWidth: Dp) {
    val dotColor = PolkadotTheme.colors.fg.tertiary.copy(alpha = SKELETON_DOT_ALPHA)

    Column(
        modifier = modifier
            .background(PolkadotTheme.colors.bg.surface.container, PolkadotTheme.shapes.extraLarge)
            .padding(PolkadotTheme.spacings.mediumIncreased)
    ) {
        SkeletonBlock(
            modifier = Modifier.size(width = labelWidth, height = SkeletonLabelHeight),
            shape = PolkadotTheme.shapes.small
        )
        VerticalSpacer { mediumIncreased }
        Box(
            modifier = Modifier
                .fillMaxSize()
                .clipToBounds()
                .drawBehind { drawDotGrid(dotColor) }
        )
    }
}

@Composable
private fun SkeletonBlock(modifier: Modifier, shape: Shape) {
    Box(modifier = modifier.background(PolkadotTheme.colors.bg.surface.nested, shape))
}

private fun DrawScope.drawDotGrid(color: Color) {
    val pitch = SkeletonDotPitch.toPx()
    val radius = SkeletonDotRadius.toPx()
    val left = (size.width - pitch * (SKELETON_DOT_COLUMNS - 1)) / 2
    var y = radius
    while (y + radius <= size.height) {
        repeat(SKELETON_DOT_COLUMNS) { column -> drawCircle(color, radius, Offset(left + column * pitch, y)) }
        y += pitch
    }
}

@Composable
private fun Modifier.skeletonSweep(running: Boolean): Modifier {
    val highlight = PolkadotTheme.colors.fg.primary.copy(alpha = SKELETON_SWEEP_ALPHA)
    val progress = remember { Animatable(0f) }
    LaunchedEffect(running) {
        if (running) progress.animateTo(1f, infiniteRepeatable(tween(SKELETON_SWEEP_LOOP_MS, easing = LinearEasing)))
    }

    return drawWithCache {
        val band = size.width * SKELETON_SWEEP_BAND_WIDTH
        val brush = Brush.horizontalGradient(listOf(Color.Transparent, highlight, Color.Transparent), endX = band)
        val bandSize = Size(band, size.height)

        onDrawWithContent {
            drawContent()
            translate(left = -band + (size.width + band) * progress.value) { drawRect(brush, size = bandSize) }
        }
    }
}
