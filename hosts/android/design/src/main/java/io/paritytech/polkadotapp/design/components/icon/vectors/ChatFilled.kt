package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ChatFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ChatFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            fill = SolidColor(Color(0xFF080808)),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(4.848f, 2.771f)
            curveTo(7.183f, 2.428f, 9.571f, 2.25f, 12f, 2.25f)
            curveTo(14.429f, 2.25f, 16.817f, 2.428f, 19.152f, 2.771f)
            curveTo(21.13f, 3.062f, 22.5f, 4.794f, 22.5f, 6.741f)
            verticalLineTo(12.759f)
            curveTo(22.5f, 14.706f, 21.13f, 16.438f, 19.152f, 16.729f)
            curveTo(17.212f, 17.014f, 15.236f, 17.185f, 13.23f, 17.235f)
            curveTo(13.127f, 17.237f, 13.032f, 17.279f, 12.964f, 17.347f)
            lineTo(8.78f, 21.53f)
            curveTo(8.566f, 21.745f, 8.243f, 21.809f, 7.963f, 21.693f)
            curveTo(7.683f, 21.577f, 7.5f, 21.303f, 7.5f, 21f)
            verticalLineTo(17.045f)
            curveTo(6.609f, 16.963f, 5.725f, 16.858f, 4.848f, 16.729f)
            curveTo(2.87f, 16.438f, 1.5f, 14.705f, 1.5f, 12.759f)
            verticalLineTo(6.741f)
            curveTo(1.5f, 4.795f, 2.87f, 3.062f, 4.848f, 2.771f)
            close()
        }
    }.build()
}
