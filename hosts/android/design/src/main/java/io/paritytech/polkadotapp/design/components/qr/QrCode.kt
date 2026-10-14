package io.paritytech.polkadotapp.design.components.qr

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import io.github.alexzhirkevich.qrose.options.QrBallShape
import io.github.alexzhirkevich.qrose.options.QrBrush
import io.github.alexzhirkevich.qrose.options.QrFrameShape
import io.github.alexzhirkevich.qrose.options.QrPixelShape
import io.github.alexzhirkevich.qrose.options.circle
import io.github.alexzhirkevich.qrose.options.solid
import io.github.alexzhirkevich.qrose.rememberQrCodePainter
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun QrCode(
    modifier: Modifier = Modifier,
    text: String
) {
    val themeColors = PolkadotTheme.colors
    val painter = rememberQrCodePainter(text) {
        shapes {
            ball = QrBallShape.circle()
            frame = QrFrameShape.circle()
            darkPixel = QrPixelShape.circle(1f)
        }
        colors {
            ball = QrBrush.solid(themeColors.fg.primary)
            frame = QrBrush.solid(themeColors.fg.primary)
            dark = QrBrush.solid(themeColors.fg.primary)
        }
    }

    Box(modifier) {
        PolkadotSurface(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(1f),
            color = Color.Transparent
        ) {
            Image(
                painter = painter,
                contentDescription = "qr_image"
            )
        }
    }
}

@Preview
@Composable
private fun QrCodePreview() {
    PolkadotTheme {
        QrCode(
            text = "https://polkadot.app"
        )
    }
}
