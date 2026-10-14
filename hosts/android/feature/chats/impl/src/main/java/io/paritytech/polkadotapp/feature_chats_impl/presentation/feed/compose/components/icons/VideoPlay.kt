package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val VideoPlay: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "VideoPlay",
        defaultWidth = 40.dp,
        defaultHeight = 40.dp,
        viewportWidth = 40f,
        viewportHeight = 40f
    ).apply {
        path(
            fill = SolidColor(Color(0xFFF4F4F5)),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(8.333f, 7.138f)
            curveTo(8.333f, 4.248f, 11.401f, 2.417f, 13.908f, 3.809f)
            lineTo(33.051f, 16.671f)
            curveTo(35.65f, 18.115f, 35.65f, 21.886f, 33.051f, 23.329f)
            lineTo(13.908f, 36.191f)
            curveTo(11.401f, 37.584f, 8.333f, 35.752f, 8.333f, 32.862f)
            verticalLineTo(7.138f)
            close()
        }
    }.build()
}
