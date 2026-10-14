package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.VisibilityOffOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "VisibilityOffOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(12f, 5.975f)
            curveTo(15.79f, 5.975f, 19.17f, 8.105f, 20.82f, 11.475f)
            curveTo(20.23f, 12.695f, 19.4f, 13.745f, 18.41f, 14.595f)
            lineTo(19.82f, 16.005f)
            curveTo(21.21f, 14.775f, 22.31f, 13.235f, 23f, 11.475f)
            curveTo(21.27f, 7.085f, 17f, 3.975f, 12f, 3.975f)
            curveTo(10.73f, 3.975f, 9.51f, 4.175f, 8.36f, 4.545f)
            lineTo(10.01f, 6.195f)
            curveTo(10.66f, 6.065f, 11.32f, 5.975f, 12f, 5.975f)
            close()
            moveTo(10.93f, 7.115f)
            lineTo(13f, 9.185f)
            curveTo(13.57f, 9.435f, 14.03f, 9.895f, 14.28f, 10.465f)
            lineTo(16.35f, 12.535f)
            curveTo(16.43f, 12.195f, 16.49f, 11.835f, 16.49f, 11.465f)
            curveTo(16.5f, 8.985f, 14.48f, 6.975f, 12f, 6.975f)
            curveTo(11.63f, 6.975f, 11.28f, 7.025f, 10.93f, 7.115f)
            close()
            moveTo(2.01f, 3.845f)
            lineTo(4.69f, 6.525f)
            curveTo(3.06f, 7.805f, 1.77f, 9.505f, 1f, 11.475f)
            curveTo(2.73f, 15.865f, 7f, 18.975f, 12f, 18.975f)
            curveTo(13.52f, 18.975f, 14.98f, 18.685f, 16.32f, 18.155f)
            lineTo(19.74f, 21.575f)
            lineTo(21.15f, 20.165f)
            lineTo(3.42f, 2.425f)
            lineTo(2.01f, 3.845f)
            close()
            moveTo(9.51f, 11.345f)
            lineTo(12.12f, 13.955f)
            curveTo(12.08f, 13.965f, 12.04f, 13.975f, 12f, 13.975f)
            curveTo(10.62f, 13.975f, 9.5f, 12.855f, 9.5f, 11.475f)
            curveTo(9.5f, 11.425f, 9.51f, 11.395f, 9.51f, 11.345f)
            close()
            moveTo(6.11f, 7.945f)
            lineTo(7.86f, 9.695f)
            curveTo(7.63f, 10.245f, 7.5f, 10.845f, 7.5f, 11.475f)
            curveTo(7.5f, 13.955f, 9.52f, 15.975f, 12f, 15.975f)
            curveTo(12.63f, 15.975f, 13.23f, 15.845f, 13.77f, 15.615f)
            lineTo(14.75f, 16.595f)
            curveTo(13.87f, 16.835f, 12.95f, 16.975f, 12f, 16.975f)
            curveTo(8.21f, 16.975f, 4.83f, 14.845f, 3.18f, 11.475f)
            curveTo(3.88f, 10.045f, 4.9f, 8.865f, 6.11f, 7.945f)
            close()
        }
    }.build()
}
