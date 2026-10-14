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

val NovaIcons.Report: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Report",
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
            moveTo(8.27f, 3f)
            lineTo(15.73f, 3f)
            lineTo(21f, 8.27f)
            lineTo(21f, 15.73f)
            lineTo(15.73f, 21f)
            lineTo(8.27f, 21f)
            lineTo(3f, 15.73f)
            lineTo(3f, 8.27f)
            lineTo(8.27f, 3f)
            close()
            moveTo(5f, 14.9f)
            lineTo(9.1f, 19f)
            lineTo(14.9f, 19f)
            lineTo(19f, 14.9f)
            lineTo(19f, 9.1f)
            lineTo(14.9f, 5f)
            lineTo(9.1f, 5f)
            lineTo(5f, 9.1f)
            lineTo(5f, 14.9f)
            close()
        }
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
            moveTo(12f, 17f)
            curveTo(11.4477f, 17f, 11f, 16.5523f, 11f, 16f)
            curveTo(11f, 15.4477f, 11.4477f, 15f, 12f, 15f)
            curveTo(12.5523f, 15f, 13f, 15.4477f, 13f, 16f)
            curveTo(13f, 16.5523f, 12.5523f, 17f, 12f, 17f)
            close()
        }
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
            moveTo(13f, 7f)
            lineTo(11f, 7f)
            lineTo(11f, 14f)
            lineTo(13f, 14f)
            lineTo(13f, 7f)
            close()
        }
    }.build()
}
