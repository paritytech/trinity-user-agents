package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.TokenUSDT: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "USDT",
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
            path(fill = SolidColor(Color(0xFF009393))) {
                moveTo(24f, 48f)
                curveTo(37.255f, 48f, 48f, 37.255f, 48f, 24f)
                curveTo(48f, 10.745f, 37.255f, 0f, 24f, 0f)
                curveTo(10.745f, 0f, 0f, 10.745f, 0f, 24f)
                curveTo(0f, 37.255f, 10.745f, 48f, 24f, 48f)
                close()
            }
            path(
                fill = SolidColor(Color.White),
                pathFillType = PathFillType.EvenOdd
            ) {
                moveTo(24.03f, 25.716f)
                curveTo(28.157f, 25.716f, 31.606f, 25.018f, 32.449f, 24.086f)
                curveTo(31.734f, 23.295f, 29.145f, 22.672f, 25.857f, 22.502f)
                verticalLineTo(24.471f)
                curveTo(25.268f, 24.502f, 24.656f, 24.517f, 24.029f, 24.517f)
                curveTo(23.402f, 24.517f, 22.79f, 24.502f, 22.2f, 24.471f)
                verticalLineTo(22.502f)
                curveTo(18.913f, 22.672f, 16.323f, 23.295f, 15.607f, 24.086f)
                curveTo(16.452f, 25.018f, 19.902f, 25.716f, 24.029f, 25.716f)
                horizontalLineTo(24.03f)
                close()
                moveTo(31.363f, 16.444f)
                verticalLineTo(19.157f)
                horizontalLineTo(25.857f)
                verticalLineTo(21.038f)
                curveTo(29.724f, 21.239f, 32.626f, 22.065f, 32.648f, 23.055f)
                verticalLineTo(25.118f)
                curveTo(32.626f, 26.107f, 29.724f, 26.932f, 25.857f, 27.134f)
                verticalLineTo(31.75f)
                horizontalLineTo(22.201f)
                verticalLineTo(27.134f)
                curveTo(18.333f, 26.933f, 15.432f, 26.107f, 15.411f, 25.118f)
                verticalLineTo(23.055f)
                curveTo(15.432f, 22.065f, 18.333f, 21.239f, 22.201f, 21.038f)
                verticalLineTo(19.157f)
                horizontalLineTo(16.695f)
                verticalLineTo(16.444f)
                horizontalLineTo(31.363f)
                horizontalLineTo(31.363f)
                close()
                moveTo(14.529f, 12.127f)
                horizontalLineTo(33.859f)
                curveTo(34.321f, 12.127f, 34.746f, 12.37f, 34.977f, 12.765f)
                lineTo(40.608f, 22.434f)
                curveTo(40.899f, 22.936f, 40.813f, 23.569f, 40.396f, 23.975f)
                lineTo(24.896f, 39.106f)
                curveTo(24.393f, 39.596f, 23.586f, 39.596f, 23.084f, 39.106f)
                lineTo(7.603f, 23.996f)
                curveTo(7.177f, 23.579f, 7.097f, 22.928f, 7.411f, 22.424f)
                lineTo(13.431f, 12.735f)
                curveTo(13.665f, 12.358f, 14.082f, 12.128f, 14.53f, 12.128f)
                lineTo(14.529f, 12.127f)
                close()
            }
        }
    }.build()
}
