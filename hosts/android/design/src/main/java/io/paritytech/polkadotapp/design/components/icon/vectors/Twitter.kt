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

val NovaIcons.Twitter: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Twitter",
        defaultWidth = 16.dp,
        defaultHeight = 16.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(
            fill = SolidColor(Color(0xFF141414)),
            fillAlpha = 1.0f,
            stroke = null,
            strokeAlpha = 1.0f,
            strokeLineWidth = 1.0f,
            strokeLineCap = StrokeCap.Butt,
            strokeLineJoin = StrokeJoin.Miter,
            strokeLineMiter = 1.0f,
            pathFillType = PathFillType.NonZero
        ) {
            moveTo(12.2176f, 1.26929f)
            horizontalLineTo(14.4666f)
            lineTo(9.55323f, 6.88495f)
            lineTo(15.3334f, 14.5266f)
            horizontalLineTo(10.8076f)
            lineTo(7.26277f, 9.89198f)
            lineTo(3.20671f, 14.5266f)
            horizontalLineTo(0.956369f)
            lineTo(6.2117f, 8.52002f)
            lineTo(0.666748f, 1.26929f)
            horizontalLineTo(5.30749f)
            lineTo(8.51168f, 5.50551f)
            lineTo(12.2176f, 1.26929f)
            close()
            moveTo(11.4283f, 13.1805f)
            horizontalLineTo(12.6745f)
            lineTo(4.63034f, 2.54471f)
            horizontalLineTo(3.29306f)
            lineTo(11.4283f, 13.1805f)
            close()
        }
    }.build()
}
