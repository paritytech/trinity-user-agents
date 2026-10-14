package io.paritytech.polkadotapp.design.components.image

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.graphics.painter.Painter
import com.vanniktech.blurhash.BlurHash

private const val PLACEHOLDER_WIDTH = 32
private const val PLACEHOLDER_HEIGHT = 32

@Composable
fun rememberBlurHashPlaceholder(blurHash: String?): Painter? {
    if (blurHash.isNullOrBlank()) return null

    return remember(blurHash) {
        val bitmap = BlurHash.decode(
            blurHash = blurHash,
            width = PLACEHOLDER_WIDTH,
            height = PLACEHOLDER_HEIGHT
        ) ?: return@remember null

        BitmapPainter(bitmap.asImageBitmap())
    }
}
