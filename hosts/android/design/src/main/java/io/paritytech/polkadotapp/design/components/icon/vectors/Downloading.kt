package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Downloading: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Downloading",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            fill = SolidColor(Color(0xFFFFFFFF)),
            fillAlpha = 1.0f,
            stroke = null,
            strokeAlpha = 1.0f,
            strokeLineWidth = 1.0f,
            strokeLineCap = StrokeCap.Butt,
            strokeLineJoin = StrokeJoin.Miter,
            strokeLineMiter = 1.0f,
            pathFillType = PathFillType.NonZero
        ) {
            moveTo(18.3449f, 4.26005f)
            curveTo(16.8649f, 3.0501f, 15.0349f, 2.25f, 13.0249f, 2.0501f)
            verticalLineTo(4.07005f)
            curveTo(14.4849f, 4.25f, 15.8149f, 4.83f, 16.9249f, 5.6901f)
            lineTo(18.3449f, 4.26005f)
            close()
            moveTo(19.9549f, 11f)
            horizontalLineTo(21.9749f)
            curveTo(21.7749f, 8.9901f, 20.9749f, 7.1601f, 19.7649f, 5.68f)
            lineTo(18.3349f, 7.10005f)
            curveTo(19.1949f, 8.2101f, 19.7749f, 9.5401f, 19.9549f, 11f)
            close()
            moveTo(18.3349f, 16.9f)
            lineTo(19.7649f, 18.33f)
            curveTo(20.9749f, 16.85f, 21.7749f, 15.01f, 21.9749f, 13.01f)
            horizontalLineTo(19.9549f)
            curveTo(19.7749f, 14.46f, 19.1949f, 15.79f, 18.3349f, 16.9f)
            close()
            moveTo(13.0249f, 19.93f)
            verticalLineTo(21.9501f)
            curveTo(15.0349f, 21.75f, 16.8649f, 20.9501f, 18.3449f, 19.74f)
            lineTo(16.9149f, 18.31f)
            curveTo(15.8149f, 19.17f, 14.4849f, 19.75f, 13.0249f, 19.93f)
            close()
            moveTo(15.6149f, 10.59f)
            lineTo(13.0249f, 13.17f)
            verticalLineTo(7.00005f)
            horizontalLineTo(11.0249f)
            verticalLineTo(13.17f)
            lineTo(8.4349f, 10.58f)
            lineTo(7.0249f, 12f)
            lineTo(12.0249f, 17f)
            lineTo(17.0249f, 12f)
            lineTo(15.6149f, 10.59f)
            close()
            moveTo(11.0249f, 19.93f)
            verticalLineTo(21.9501f)
            curveTo(5.9749f, 21.4501f, 2.0249f, 17.19f, 2.0249f, 12f)
            curveTo(2.0249f, 6.8101f, 5.9749f, 2.5501f, 11.0249f, 2.0501f)
            verticalLineTo(4.07005f)
            curveTo(7.0749f, 4.5601f, 4.0249f, 7.92f, 4.0249f, 12f)
            curveTo(4.0249f, 16.08f, 7.0749f, 19.4401f, 11.0249f, 19.93f)
            close()
        }
    }.build()
}
