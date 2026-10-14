package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.MoneyFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MoneyFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 2f)
            curveTo(6.48f, 2f, 2f, 6.48f, 2f, 12f)
            curveTo(2f, 17.52f, 6.48f, 22f, 12f, 22f)
            curveTo(17.52f, 22f, 22f, 17.52f, 22f, 12f)
            curveTo(22f, 6.48f, 17.52f, 2f, 12f, 2f)
            close()
            moveTo(12.88f, 17.76f)
            verticalLineTo(19f)
            horizontalLineTo(11.13f)
            verticalLineTo(17.71f)
            curveTo(10.39f, 17.53f, 8.74f, 16.94f, 8.11f, 14.75f)
            lineTo(9.76f, 14.08f)
            curveTo(9.82f, 14.3f, 10.34f, 16.17f, 12.16f, 16.17f)
            curveTo(13.09f, 16.17f, 14.14f, 15.69f, 14.14f, 14.56f)
            curveTo(14.14f, 13.6f, 13.44f, 13.1f, 11.86f, 12.53f)
            curveTo(10.76f, 12.14f, 8.51f, 11.5f, 8.51f, 9.22f)
            curveTo(8.51f, 9.12f, 8.52f, 6.82f, 11.13f, 6.26f)
            verticalLineTo(5f)
            horizontalLineTo(12.88f)
            verticalLineTo(6.24f)
            curveTo(14.72f, 6.56f, 15.39f, 8.03f, 15.54f, 8.47f)
            lineTo(13.96f, 9.14f)
            curveTo(13.85f, 8.79f, 13.37f, 7.8f, 12.06f, 7.8f)
            curveTo(11.36f, 7.8f, 10.25f, 8.17f, 10.25f, 9.19f)
            curveTo(10.25f, 10.14f, 11.11f, 10.5f, 12.89f, 11.09f)
            curveTo(15.29f, 11.92f, 15.9f, 13.14f, 15.9f, 14.54f)
            curveTo(15.9f, 17.17f, 13.4f, 17.67f, 12.88f, 17.76f)
            close()
        }
    }.build()
}
