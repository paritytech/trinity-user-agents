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

val NovaIcons.ReplyArrow: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ReplyArrow",
        defaultWidth = 24.dp,
        defaultHeight = 20.dp,
        viewportWidth = 24f,
        viewportHeight = 20f
    ).apply {
        path(
            fill = SolidColor(Color(0xFFFFFFFF)), // #ffffff
            stroke = null,
            strokeLineWidth = 0.0f,
            strokeLineCap = StrokeCap.Butt,
            strokeLineJoin = StrokeJoin.Miter,
            strokeLineMiter = 4.0f,
            pathFillType = PathFillType.NonZero
        ) {
            moveTo(9.333f, 5.333f)
            verticalLineTo(0f)
            lineTo(0f, 9.333f)
            lineTo(9.333f, 18.667f)
            verticalLineTo(13.2f)
            curveTo(16f, 13.2f, 20.667f, 15.333f, 24f, 20f)
            curveTo(22.667f, 13.333f, 18.667f, 6.667f, 9.333f, 5.333f)
            close()
        }
    }.build()
}
