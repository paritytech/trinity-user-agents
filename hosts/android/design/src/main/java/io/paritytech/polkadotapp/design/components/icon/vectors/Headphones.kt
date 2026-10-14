package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Headphones: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Headphones",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(12f, 1f)
            curveToRelative(-4.97f, 0f, -9f, 4.03f, -9f, 9f)
            verticalLineToRelative(7f)
            curveToRelative(0f, 1.66f, 1.34f, 3f, 3f, 3f)
            horizontalLineToRelative(3f)
            verticalLineToRelative(-8f)
            horizontalLineTo(5f)
            verticalLineToRelative(-2f)
            curveToRelative(0f, -3.87f, 3.13f, -7f, 7f, -7f)
            reflectiveCurveToRelative(7f, 3.13f, 7f, 7f)
            verticalLineToRelative(2f)
            horizontalLineToRelative(-4f)
            verticalLineToRelative(8f)
            horizontalLineToRelative(3f)
            curveToRelative(1.66f, 0f, 3f, -1.34f, 3f, -3f)
            verticalLineToRelative(-7f)
            curveToRelative(0f, -4.97f, -4.03f, -9f, -9f, -9f)
            close()
        }
    }.build()
}
