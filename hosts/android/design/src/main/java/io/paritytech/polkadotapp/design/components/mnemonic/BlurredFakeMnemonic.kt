package io.paritytech.polkadotapp.design.components.mnemonic

import android.graphics.BlurMaskFilter
import android.graphics.Paint
import androidx.compose.foundation.layout.Spacer
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb

private const val CoverColumns = 3
private const val CoverRows = 4

private const val HorizontalPaddingFactor = 0.05f
private const val VerticalPaddingFactor = 0.06f
private const val ColumnGapFactor = 0.03f
private const val RowGapFactor = 0.04f

private const val RadiusFactor = 0.45f
private const val BlurRadiusFactor = 0.2f

@Composable
internal fun BlurredFakeMnemonic(
    modifier: Modifier,
    surfaceColor: Color,
    wordColor: Color
) {
    Spacer(
        modifier = modifier.drawWithCache {
            val horizontalPadding = size.width * HorizontalPaddingFactor
            val verticalPadding = size.height * VerticalPaddingFactor
            val columnGap = size.width * ColumnGapFactor
            val rowGap = size.height * RowGapFactor
            val columnWidth =
                (size.width - horizontalPadding * 2 - columnGap * (CoverColumns - 1)) / CoverColumns
            val pillHeight =
                (size.height - verticalPadding * 2 - rowGap * (CoverRows - 1)) / CoverRows
            val radius = pillHeight * RadiusFactor
            val blurRadius = pillHeight * BlurRadiusFactor

            val paint = Paint().apply {
                isAntiAlias = true
                color = wordColor.toArgb()
                if (blurRadius > 0) {
                    maskFilter = BlurMaskFilter(blurRadius, BlurMaskFilter.Blur.NORMAL)
                }
            }

            onDrawBehind {
                drawRect(color = surfaceColor)
                drawIntoCanvas { canvas ->
                    repeat(CoverColumns * CoverRows) { index ->
                        val column = index % CoverColumns
                        val row = index / CoverColumns
                        val left = horizontalPadding + column * (columnWidth + columnGap)
                        val top = verticalPadding + row * (pillHeight + rowGap)
                        canvas.nativeCanvas.drawRoundRect(
                            left,
                            top,
                            left + columnWidth,
                            top + pillHeight,
                            radius,
                            radius,
                            paint
                        )
                    }
                }
            }
        }
    )
}
