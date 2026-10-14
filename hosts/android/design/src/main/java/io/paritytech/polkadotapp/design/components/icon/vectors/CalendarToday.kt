package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CalendarToday: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CalendarToday",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(20f, 3f)
            horizontalLineTo(19f)
            verticalLineTo(1f)
            horizontalLineTo(17f)
            verticalLineTo(3f)
            horizontalLineTo(7f)
            verticalLineTo(1f)
            horizontalLineTo(5f)
            verticalLineTo(3f)
            horizontalLineTo(4f)
            curveTo(2.9f, 3f, 2f, 3.9f, 2f, 5f)
            verticalLineTo(21f)
            curveTo(2f, 22.1f, 2.9f, 23f, 4f, 23f)
            horizontalLineTo(20f)
            curveTo(21.1f, 23f, 22f, 22.1f, 22f, 21f)
            verticalLineTo(5f)
            curveTo(22f, 3.9f, 21.1f, 3f, 20f, 3f)
            close()
            moveTo(20f, 21f)
            horizontalLineTo(4f)
            verticalLineTo(10f)
            horizontalLineTo(20f)
            verticalLineTo(21f)
            close()
            moveTo(20f, 8f)
            horizontalLineTo(4f)
            verticalLineTo(5f)
            horizontalLineTo(20f)
            verticalLineTo(8f)
            close()
        }
    }.build()
}
