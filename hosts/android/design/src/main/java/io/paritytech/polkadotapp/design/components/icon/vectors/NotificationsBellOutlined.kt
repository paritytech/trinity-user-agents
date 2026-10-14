package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.NotificationsBellOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NotificationsOutlined",
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
            moveTo(10.268f, 21f)
            curveTo(10.444f, 21.304f, 10.696f, 21.556f, 11f, 21.732f)
            curveTo(11.304f, 21.907f, 11.649f, 22f, 12f, 22f)
            curveTo(12.351f, 22f, 12.696f, 21.907f, 13f, 21.732f)
            curveTo(13.304f, 21.556f, 13.557f, 21.304f, 13.732f, 21f)
            moveTo(3.262f, 15.326f)
            curveTo(3.132f, 15.469f, 3.045f, 15.647f, 3.014f, 15.839f)
            curveTo(2.983f, 16.03f, 3.008f, 16.226f, 3.086f, 16.403f)
            curveTo(3.164f, 16.581f, 3.292f, 16.732f, 3.454f, 16.837f)
            curveTo(3.617f, 16.943f, 3.806f, 17f, 4f, 17f)
            horizontalLineTo(20f)
            curveTo(20.194f, 17f, 20.384f, 16.944f, 20.546f, 16.838f)
            curveTo(20.708f, 16.732f, 20.837f, 16.582f, 20.915f, 16.404f)
            curveTo(20.993f, 16.227f, 21.019f, 16.031f, 20.988f, 15.84f)
            curveTo(20.957f, 15.649f, 20.871f, 15.47f, 20.74f, 15.327f)
            curveTo(19.41f, 13.956f, 18f, 12.499f, 18f, 8f)
            curveTo(18f, 6.409f, 17.368f, 4.883f, 16.243f, 3.757f)
            curveTo(15.118f, 2.632f, 13.592f, 2f, 12f, 2f)
            curveTo(10.409f, 2f, 8.883f, 2.632f, 7.758f, 3.757f)
            curveTo(6.632f, 4.883f, 6f, 6.409f, 6f, 8f)
            curveTo(6f, 12.499f, 4.589f, 13.956f, 3.262f, 15.326f)
            close()
        }
    }.build()
}
