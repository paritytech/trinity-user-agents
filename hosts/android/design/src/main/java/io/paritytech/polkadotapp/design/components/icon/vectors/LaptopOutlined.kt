package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.LaptopOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "LaptopOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            stroke = SolidColor(Color(0xFF080808)),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            moveTo(2f, 20f)
            horizontalLineTo(22f)
            moveTo(5f, 4f)
            horizontalLineTo(19f)
            curveTo(20.105f, 4f, 21f, 4.895f, 21f, 6f)
            verticalLineTo(14f)
            curveTo(21f, 15.105f, 20.105f, 16f, 19f, 16f)
            horizontalLineTo(5f)
            curveTo(3.895f, 16f, 3f, 15.105f, 3f, 14f)
            verticalLineTo(6f)
            curveTo(3f, 4.895f, 3.895f, 4f, 5f, 4f)
            close()
        }
    }.build()
}
