package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Search: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Search",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(15.755f, 14.255f)
            horizontalLineTo(14.965f)
            lineTo(14.685f, 13.985f)
            curveTo(15.665f, 12.845f, 16.255f, 11.365f, 16.255f, 9.755f)
            curveTo(16.255f, 6.165f, 13.345f, 3.255f, 9.755f, 3.255f)
            curveTo(6.165f, 3.255f, 3.255f, 6.165f, 3.255f, 9.755f)
            curveTo(3.255f, 13.345f, 6.165f, 16.255f, 9.755f, 16.255f)
            curveTo(11.365f, 16.255f, 12.845f, 15.665f, 13.985f, 14.685f)
            lineTo(14.255f, 14.965f)
            verticalLineTo(15.755f)
            lineTo(19.255f, 20.745f)
            lineTo(20.745f, 19.255f)
            lineTo(15.755f, 14.255f)
            close()
            moveTo(9.755f, 14.255f)
            curveTo(7.265f, 14.255f, 5.255f, 12.245f, 5.255f, 9.755f)
            curveTo(5.255f, 7.265f, 7.265f, 5.255f, 9.755f, 5.255f)
            curveTo(12.245f, 5.255f, 14.255f, 7.265f, 14.255f, 9.755f)
            curveTo(14.255f, 12.245f, 12.245f, 14.255f, 9.755f, 14.255f)
            close()
        }
    }.build()
}
