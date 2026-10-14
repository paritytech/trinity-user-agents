package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Success: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "IconElement",
        defaultWidth = 80.dp,
        defaultHeight = 80.dp,
        viewportWidth = 80f,
        viewportHeight = 80f
    ).apply {
        path(fill = SolidColor(Color(0xFF56F39A))) {
            moveTo(40f, 40f)
            moveToRelative(-40f, 0f)
            arcToRelative(40f, 40f, 0f, isMoreThanHalf = true, isPositiveArc = true, 80f, 0f)
            arcToRelative(40f, 40f, 0f, isMoreThanHalf = true, isPositiveArc = true, -80f, 0f)
        }
        path(
            fill = SolidColor(Color(0xFF11041F)),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(55.972f, 26.729f)
            curveTo(56.674f, 27.394f, 56.704f, 28.501f, 56.04f, 29.203f)
            lineTo(33.988f, 52.495f)
            curveTo(32.934f, 53.609f, 31.161f, 53.609f, 30.106f, 52.495f)
            lineTo(23.96f, 46.003f)
            curveTo(23.295f, 45.301f, 23.326f, 44.194f, 24.028f, 43.529f)
            curveTo(24.729f, 42.865f, 25.837f, 42.895f, 26.501f, 43.597f)
            lineTo(32.047f, 49.455f)
            lineTo(53.498f, 26.797f)
            curveTo(54.163f, 26.095f, 55.271f, 26.065f, 55.972f, 26.729f)
            close()
        }
    }.build()
}
