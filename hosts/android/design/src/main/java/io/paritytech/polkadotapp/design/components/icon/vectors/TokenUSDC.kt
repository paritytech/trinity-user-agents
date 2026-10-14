package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.TokenUSDC: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "USDC",
        defaultWidth = 48.dp,
        defaultHeight = 48.dp,
        viewportWidth = 48f,
        viewportHeight = 48f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(0f, 0f)
                horizontalLineToRelative(48f)
                verticalLineToRelative(48f)
                horizontalLineToRelative(-48f)
                close()
            }
        ) {
            path(fill = SolidColor(Color(0xFF2775CA))) {
                moveTo(24f, 48f)
                curveTo(37.3f, 48f, 48f, 37.3f, 48f, 24f)
                curveTo(48f, 10.7f, 37.3f, 0f, 24f, 0f)
                curveTo(10.7f, 0f, 0f, 10.7f, 0f, 24f)
                curveTo(0f, 37.3f, 10.7f, 48f, 24f, 48f)
                close()
            }
            path(fill = SolidColor(Color.White)) {
                moveTo(30.6f, 27.8f)
                curveTo(30.6f, 24.3f, 28.5f, 23.1f, 24.3f, 22.6f)
                curveTo(21.3f, 22.2f, 20.7f, 21.4f, 20.7f, 20f)
                curveTo(20.7f, 18.6f, 21.7f, 17.7f, 23.7f, 17.7f)
                curveTo(25.5f, 17.7f, 26.5f, 18.3f, 27f, 19.8f)
                curveTo(27.1f, 20.1f, 27.4f, 20.3f, 27.7f, 20.3f)
                horizontalLineTo(29.3f)
                curveTo(29.7f, 20.3f, 30f, 20f, 30f, 19.6f)
                verticalLineTo(19.5f)
                curveTo(29.6f, 17.3f, 27.8f, 15.6f, 25.5f, 15.4f)
                verticalLineTo(13f)
                curveTo(25.5f, 12.6f, 25.2f, 12.3f, 24.7f, 12.2f)
                horizontalLineTo(23.2f)
                curveTo(22.8f, 12.2f, 22.5f, 12.5f, 22.4f, 13f)
                verticalLineTo(15.3f)
                curveTo(19.4f, 15.7f, 17.5f, 17.7f, 17.5f, 20.2f)
                curveTo(17.5f, 23.5f, 19.5f, 24.8f, 23.7f, 25.3f)
                curveTo(26.5f, 25.8f, 27.4f, 26.4f, 27.4f, 28f)
                curveTo(27.4f, 29.6f, 26f, 30.7f, 24.1f, 30.7f)
                curveTo(21.5f, 30.7f, 20.6f, 29.6f, 20.3f, 28.1f)
                curveTo(20.2f, 27.7f, 19.9f, 27.5f, 19.6f, 27.5f)
                horizontalLineTo(17.9f)
                curveTo(17.5f, 27.5f, 17.2f, 27.8f, 17.2f, 28.2f)
                verticalLineTo(28.3f)
                curveTo(17.6f, 30.8f, 19.2f, 32.6f, 22.5f, 33.1f)
                verticalLineTo(35.5f)
                curveTo(22.5f, 35.9f, 22.8f, 36.2f, 23.3f, 36.3f)
                horizontalLineTo(24.8f)
                curveTo(25.2f, 36.3f, 25.5f, 36f, 25.6f, 35.5f)
                verticalLineTo(33.1f)
                curveTo(28.6f, 32.6f, 30.6f, 30.5f, 30.6f, 27.8f)
                close()
            }
            path(fill = SolidColor(Color.White)) {
                moveTo(18.9f, 38.3f)
                curveTo(11.1f, 35.5f, 7.1f, 26.8f, 10f, 19.1f)
                curveTo(11.5f, 14.9f, 14.8f, 11.7f, 18.9f, 10.2f)
                curveTo(19.3f, 10f, 19.5f, 9.7f, 19.5f, 9.2f)
                verticalLineTo(7.8f)
                curveTo(19.5f, 7.4f, 19.3f, 7.1f, 18.9f, 7f)
                curveTo(18.8f, 7f, 18.6f, 7f, 18.5f, 7.1f)
                curveTo(9f, 10.1f, 3.8f, 20.2f, 6.8f, 29.7f)
                curveTo(8.6f, 35.3f, 12.9f, 39.6f, 18.5f, 41.4f)
                curveTo(18.9f, 41.6f, 19.3f, 41.4f, 19.4f, 41f)
                curveTo(19.5f, 40.9f, 19.5f, 40.8f, 19.5f, 40.6f)
                verticalLineTo(39.2f)
                curveTo(19.5f, 38.9f, 19.2f, 38.5f, 18.9f, 38.3f)
                close()
                moveTo(29.5f, 7.1f)
                curveTo(29.1f, 6.9f, 28.7f, 7.1f, 28.6f, 7.5f)
                curveTo(28.5f, 7.6f, 28.5f, 7.7f, 28.5f, 7.9f)
                verticalLineTo(9.3f)
                curveTo(28.5f, 9.7f, 28.8f, 10.1f, 29.1f, 10.3f)
                curveTo(36.9f, 13.1f, 40.9f, 21.8f, 38f, 29.5f)
                curveTo(36.5f, 33.7f, 33.2f, 36.9f, 29.1f, 38.4f)
                curveTo(28.7f, 38.6f, 28.5f, 38.9f, 28.5f, 39.4f)
                verticalLineTo(40.8f)
                curveTo(28.5f, 41.2f, 28.7f, 41.5f, 29.1f, 41.6f)
                curveTo(29.2f, 41.6f, 29.4f, 41.6f, 29.5f, 41.5f)
                curveTo(39f, 38.5f, 44.2f, 28.4f, 41.2f, 18.9f)
                curveTo(39.4f, 13.2f, 35f, 8.9f, 29.5f, 7.1f)
                close()
            }
        }
    }.build()
}
