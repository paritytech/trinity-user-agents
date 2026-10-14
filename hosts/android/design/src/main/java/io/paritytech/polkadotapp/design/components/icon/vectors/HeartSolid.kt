package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.HeartSolid: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "HeartSolid",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Glyph authored on a 16-unit grid; scaled into the 24dp design-system canvas.
        group(scaleX = 1.5f, scaleY = 1.5f) {
            path(fill = SolidColor(Color(0xff000000))) {
                moveTo(7.711f, 14.226f)
                lineTo(7.706f, 14.223f)
                lineTo(7.691f, 14.215f)
                curveTo(7.678f, 14.208f, 7.659f, 14.198f, 7.635f, 14.185f)
                curveTo(7.588f, 14.158f, 7.519f, 14.119f, 7.433f, 14.069f)
                curveTo(7.26f, 13.969f, 7.015f, 13.821f, 6.722f, 13.631f)
                curveTo(6.138f, 13.251f, 5.356f, 12.697f, 4.571f, 12.002f)
                curveTo(3.037f, 10.642f, 1.333f, 8.601f, 1.333f, 6.157f)
                curveTo(1.333f, 4.027f, 3.012f, 2.3f, 5.083f, 2.3f)
                curveTo(6.262f, 2.3f, 7.313f, 2.859f, 8f, 3.733f)
                curveTo(8.687f, 2.859f, 9.738f, 2.3f, 10.917f, 2.3f)
                curveTo(12.988f, 2.3f, 14.667f, 4.027f, 14.667f, 6.157f)
                curveTo(14.667f, 8.601f, 12.963f, 10.642f, 11.429f, 12.002f)
                curveTo(10.644f, 12.697f, 9.862f, 13.251f, 9.278f, 13.631f)
                curveTo(8.985f, 13.821f, 8.74f, 13.969f, 8.567f, 14.069f)
                curveTo(8.481f, 14.119f, 8.412f, 14.158f, 8.365f, 14.185f)
                curveTo(8.341f, 14.198f, 8.322f, 14.208f, 8.309f, 14.215f)
                lineTo(8.294f, 14.223f)
                lineTo(8.289f, 14.226f)
                lineTo(8.288f, 14.227f)
                curveTo(8.108f, 14.325f, 7.892f, 14.325f, 7.713f, 14.227f)
                lineTo(7.711f, 14.226f)
                close()
            }
        }
    }.build()
}
