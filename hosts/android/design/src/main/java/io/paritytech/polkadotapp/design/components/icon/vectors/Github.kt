package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Github: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Github",
        defaultWidth = 16.dp,
        defaultHeight = 16.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        group {
            path(
                fill = SolidColor(Color(0xFF141414)),
                fillAlpha = 1.0f,
                stroke = null,
                strokeAlpha = 1.0f,
                strokeLineWidth = 1.0f,
                strokeLineCap = StrokeCap.Butt,
                strokeLineJoin = StrokeJoin.Miter,
                strokeLineMiter = 1.0f,
                pathFillType = PathFillType.EvenOdd
            ) {
                moveTo(8.00662f, 0f)
                curveTo(3.5792f, 0f, 0f, 3.6056f, 0f, 8.0661f)
                curveTo(0f, 11.6317f, 2.2933f, 14.6498f, 5.4747f, 15.7181f)
                curveTo(5.8725f, 15.7984f, 6.0182f, 15.5445f, 6.0182f, 15.331f)
                curveTo(6.0182f, 15.144f, 6.005f, 14.503f, 6.005f, 13.8352f)
                curveTo(3.7778f, 14.316f, 3.314f, 12.8736f, 3.314f, 12.8736f)
                curveTo(2.9561f, 11.9388f, 2.4257f, 11.6985f, 2.4257f, 11.6985f)
                curveTo(1.6967f, 11.2044f, 2.4788f, 11.2044f, 2.4788f, 11.2044f)
                curveTo(3.2874f, 11.2578f, 3.7117f, 12.0324f, 3.7117f, 12.0324f)
                curveTo(4.4275f, 13.2609f, 5.5807f, 12.9138f, 6.0447f, 12.7001f)
                curveTo(6.1109f, 12.1792f, 6.3232f, 11.8187f, 6.5485f, 11.6184f)
                curveTo(4.7721f, 11.4314f, 2.9031f, 10.737f, 2.9031f, 7.6387f)
                curveTo(2.9031f, 6.7573f, 3.2211f, 6.0362f, 3.7249f, 5.4754f)
                curveTo(3.6454f, 5.2751f, 3.3669f, 4.447f, 3.8045f, 3.3386f)
                curveTo(3.8045f, 3.3386f, 4.4806f, 3.1249f, 6.0049f, 4.1665f)
                curveTo(6.6575f, 3.99f, 7.3305f, 3.9002f, 8.0066f, 3.8994f)
                curveTo(8.6827f, 3.8994f, 9.3718f, 3.993f, 10.0082f, 4.1665f)
                curveTo(11.5327f, 3.1249f, 12.2087f, 3.3386f, 12.2087f, 3.3386f)
                curveTo(12.6463f, 4.447f, 12.3677f, 5.2751f, 12.2882f, 5.4754f)
                curveTo(12.8053f, 6.0362f, 13.1101f, 6.7573f, 13.1101f, 7.6387f)
                curveTo(13.1101f, 10.737f, 11.2411f, 11.418f, 9.4515f, 11.6184f)
                curveTo(9.7432f, 11.8721f, 9.9949f, 12.3528f, 9.9949f, 13.114f)
                curveTo(9.9949f, 14.1957f, 9.9818f, 15.0638f, 9.9818f, 15.3308f)
                curveTo(9.9818f, 15.5445f, 10.1277f, 15.7984f, 10.5253f, 15.7182f)
                curveTo(13.7067f, 14.6497f, 16f, 11.6317f, 16f, 8.0661f)
                curveTo(16.0131f, 3.6056f, 12.4208f, 0f, 8.0066f, 0f)
                close()
            }
        }
    }.build()
}
