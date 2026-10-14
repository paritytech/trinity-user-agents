package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.SpeakerXMark: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "SpeakerXMark",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Glyph authored on a 20-unit grid; scaled into the 24dp design-system canvas.
        group(scaleX = 1.2f, scaleY = 1.2f) {
            path(fill = SolidColor(Color(0xff000000))) {
                moveTo(10.051f, 2.566f)
                curveTo(10.347f, 2.693f, 10.539f, 2.983f, 10.539f, 3.304f)
                verticalLineTo(16.696f)
                curveTo(10.539f, 17.017f, 10.347f, 17.307f, 10.051f, 17.434f)
                curveTo(9.755f, 17.561f, 9.411f, 17.501f, 9.176f, 17.282f)
                lineTo(4.826f, 13.214f)
                horizontalLineTo(2.092f)
                curveTo(1.758f, 13.214f, 1.458f, 13.01f, 1.338f, 12.7f)
                curveTo(1.012f, 11.861f, 0.833f, 10.95f, 0.833f, 10f)
                curveTo(0.833f, 9.05f, 1.012f, 8.139f, 1.338f, 7.3f)
                curveTo(1.458f, 6.99f, 1.758f, 6.786f, 2.092f, 6.786f)
                horizontalLineTo(4.826f)
                lineTo(9.176f, 2.718f)
                curveTo(9.411f, 2.499f, 9.755f, 2.439f, 10.051f, 2.566f)
                close()
            }
            path(fill = SolidColor(Color(0xff000000))) {
                moveTo(14.077f, 7.021f)
                curveTo(13.761f, 6.707f, 13.249f, 6.707f, 12.933f, 7.021f)
                curveTo(12.617f, 7.335f, 12.617f, 7.844f, 12.933f, 8.158f)
                lineTo(14.788f, 10f)
                lineTo(12.933f, 11.842f)
                curveTo(12.617f, 12.156f, 12.617f, 12.665f, 12.933f, 12.979f)
                curveTo(13.249f, 13.293f, 13.761f, 13.293f, 14.077f, 12.979f)
                lineTo(15.931f, 11.136f)
                lineTo(17.786f, 12.979f)
                curveTo(18.102f, 13.293f, 18.614f, 13.293f, 18.93f, 12.979f)
                curveTo(19.246f, 12.665f, 19.246f, 12.156f, 18.93f, 11.842f)
                lineTo(17.075f, 10f)
                lineTo(18.93f, 8.158f)
                curveTo(19.246f, 7.844f, 19.246f, 7.335f, 18.93f, 7.021f)
                curveTo(18.614f, 6.707f, 18.102f, 6.707f, 17.786f, 7.021f)
                lineTo(15.931f, 8.864f)
                lineTo(14.077f, 7.021f)
                close()
            }
        }
    }.build()
}
