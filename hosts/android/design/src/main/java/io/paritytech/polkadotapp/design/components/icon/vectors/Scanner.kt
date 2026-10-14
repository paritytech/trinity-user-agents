package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Scanner: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Scanner",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            fill = SolidColor(Color.White),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(3f, 5f)
            curveTo(3f, 3.895f, 3.895f, 3f, 5f, 3f)
            horizontalLineTo(9f)
            curveTo(9.552f, 3f, 10f, 2.552f, 10f, 2f)
            curveTo(10f, 1.448f, 9.552f, 1f, 9f, 1f)
            horizontalLineTo(5f)
            curveTo(2.791f, 1f, 1f, 2.791f, 1f, 5f)
            verticalLineTo(9f)
            curveTo(1f, 9.552f, 1.448f, 10f, 2f, 10f)
            curveTo(2.552f, 10f, 3f, 9.552f, 3f, 9f)
            verticalLineTo(5f)
            close()
            moveTo(21f, 19f)
            curveTo(21f, 20.105f, 20.105f, 21f, 19f, 21f)
            horizontalLineTo(15f)
            curveTo(14.448f, 21f, 14f, 21.448f, 14f, 22f)
            curveTo(14f, 22.552f, 14.448f, 23f, 15f, 23f)
            horizontalLineTo(19f)
            curveTo(21.209f, 23f, 23f, 21.209f, 23f, 19f)
            verticalLineTo(16f)
            curveTo(23f, 15.448f, 22.552f, 15f, 22f, 15f)
            curveTo(21.448f, 15f, 21f, 15.448f, 21f, 16f)
            verticalLineTo(19f)
            close()
            moveTo(21f, 5f)
            curveTo(21f, 3.895f, 20.105f, 3f, 19f, 3f)
            lineTo(15f, 3f)
            curveTo(14.448f, 3f, 14f, 2.552f, 14f, 2f)
            curveTo(14f, 1.448f, 14.448f, 1f, 15f, 1f)
            horizontalLineTo(19f)
            curveTo(21.209f, 1f, 23f, 2.791f, 23f, 5f)
            verticalLineTo(9f)
            curveTo(23f, 9.552f, 22.552f, 10f, 22f, 10f)
            curveTo(21.448f, 10f, 21f, 9.552f, 21f, 9f)
            verticalLineTo(5f)
            close()
            moveTo(5f, 21f)
            curveTo(3.895f, 21f, 3f, 20.105f, 3f, 19f)
            lineTo(3f, 16f)
            curveTo(3f, 15.448f, 2.552f, 15f, 2f, 15f)
            curveTo(1.448f, 15f, 1f, 15.448f, 1f, 16f)
            verticalLineTo(19f)
            curveTo(1f, 21.209f, 2.791f, 23f, 5f, 23f)
            horizontalLineTo(9f)
            curveTo(9.552f, 23f, 10f, 22.552f, 10f, 22f)
            curveTo(10f, 21.448f, 9.552f, 21f, 9f, 21f)
            horizontalLineTo(5f)
            close()
        }
    }.build()
}
