package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ComputerDesktop: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ComputerDesktop",
        defaultWidth = 20.dp,
        defaultHeight = 20.dp,
        viewportWidth = 20f,
        viewportHeight = 20f
    ).apply {
        path(
            fill = SolidColor(Color.White),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(2f, 4.25f)
            curveTo(2f, 3.007f, 3.007f, 2f, 4.25f, 2f)
            horizontalLineTo(15.75f)
            curveTo(16.993f, 2f, 18f, 3.007f, 18f, 4.25f)
            verticalLineTo(12.75f)
            curveTo(18f, 13.993f, 16.993f, 15f, 15.75f, 15f)
            horizontalLineTo(12.645f)
            curveTo(12.842f, 15.662f, 13.229f, 16.242f, 13.745f, 16.677f)
            curveTo(13.986f, 16.88f, 14.074f, 17.212f, 13.966f, 17.507f)
            curveTo(13.858f, 17.803f, 13.576f, 18f, 13.261f, 18f)
            horizontalLineTo(6.739f)
            curveTo(6.424f, 18f, 6.142f, 17.803f, 6.034f, 17.507f)
            curveTo(5.926f, 17.212f, 6.014f, 16.88f, 6.255f, 16.677f)
            curveTo(6.771f, 16.242f, 7.158f, 15.662f, 7.355f, 15f)
            horizontalLineTo(4.25f)
            curveTo(3.007f, 15f, 2f, 13.993f, 2f, 12.75f)
            verticalLineTo(4.25f)
            close()
            moveTo(3.5f, 4.25f)
            curveTo(3.5f, 3.836f, 3.836f, 3.5f, 4.25f, 3.5f)
            horizontalLineTo(15.75f)
            curveTo(16.164f, 3.5f, 16.5f, 3.836f, 16.5f, 4.25f)
            verticalLineTo(11.75f)
            curveTo(16.5f, 12.164f, 16.164f, 12.5f, 15.75f, 12.5f)
            horizontalLineTo(4.25f)
            curveTo(3.836f, 12.5f, 3.5f, 12.164f, 3.5f, 11.75f)
            verticalLineTo(4.25f)
            close()
        }
    }.build()
}
