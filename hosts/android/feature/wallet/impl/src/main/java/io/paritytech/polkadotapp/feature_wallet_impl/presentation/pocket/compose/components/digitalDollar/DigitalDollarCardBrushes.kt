package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import android.graphics.Matrix
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RadialGradientShader
import androidx.compose.ui.graphics.Shader
import androidx.compose.ui.graphics.ShaderBrush
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors

// Fading this to transparent instead would stop drawing the bottom and trailing edges.
internal fun digitalDollarBorderBrush(litColor: Color) = Brush.linearGradient(
    colorStops = arrayOf(
        0f to litColor,
        BORDER_SHADED_STOP to BorderShaded
    )
)

// Gradient transform of design node 1733:57247, normalised from its 371.72x233.52 frame.
internal fun digitalDollarHighlightBrush() = object : ShaderBrush() {
    override fun createShader(size: Size): Shader = RadialGradientShader(
        center = Offset.Zero,
        radius = 1f,
        colors = listOf(
            PocketCardColors.Primary.copy(alpha = HIGHLIGHT_NEAR_ALPHA),
            PocketCardColors.Primary.copy(alpha = HIGHLIGHT_MID_ALPHA),
            PocketCardColors.Primary.copy(alpha = 0f)
        ),
        colorStops = HighlightStops
    ).apply {
        setLocalMatrix(
            Matrix().apply {
                setValues(
                    floatArrayOf(
                        size.width, -size.width, 0f,
                        size.height, size.height * HIGHLIGHT_ACROSS_SCALE, 0f,
                        0f, 0f, 1f
                    )
                )
            }
        )
    }
}

private val BorderShaded = Color(0x80EFEDED)

private val HighlightStops = listOf(0f, 0.5f, 1f)

private const val BORDER_SHADED_STOP = 0.37f

private const val HIGHLIGHT_NEAR_ALPHA = 0.31f

private const val HIGHLIGHT_MID_ALPHA = 0.14f

private const val HIGHLIGHT_ACROSS_SCALE = 0.39f
