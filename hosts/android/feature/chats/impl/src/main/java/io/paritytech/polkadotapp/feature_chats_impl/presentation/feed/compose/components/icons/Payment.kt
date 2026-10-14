package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val Payment: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Payment",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        group(translationX = 5.5f, translationY = 1f) {
            path(fill = SolidColor(Color.White)) {
                moveTo(5.5f, 21f)
                verticalLineTo(18.6445f)
                curveTo(3.60535f, 18.443f, 1.74831f, 17.6624f, 0.292969f, 16.207f)
                curveTo(-0.0975556f, 15.8165f, -0.0975556f, 15.1835f, 0.292969f, 14.793f)
                curveTo(0.683493f, 14.4024f, 1.31651f, 14.4024f, 1.70703f, 14.793f)
                curveTo(2.99597f, 16.0819f, 4.72998f, 16.7002f, 6.5f, 16.7002f)
                curveTo(7.4985f, 16.7002f, 8.70862f, 16.6433f, 9.64648f, 16.2891f)
                curveTo(10.102f, 16.117f, 10.4329f, 15.8969f, 10.6475f, 15.6338f)
                curveTo(10.849f, 15.3867f, 11f, 15.0397f, 11f, 14.5f)
                curveTo(10.9999f, 13.7181f, 10.682f, 13.1589f, 10.0283f, 12.7461f)
                curveTo(9.31603f, 12.2964f, 8.15679f, 12.0001f, 6.5f, 12f)
                curveTo(4.6224f, 12f, 3.03405f, 11.5994f, 1.88281f, 10.832f)
                curveTo(0.705666f, 10.0473f, 0f, 8.87808f, 0f, 7.5f)
                curveTo(0f, 5.55628f, 1.1792f, 4.38669f, 2.52246f, 3.76172f)
                curveTo(3.46777f, 3.32194f, 4.54438f, 3.11911f, 5.5f, 3.04102f)
                verticalLineTo(1f)
                curveTo(5.5f, 0.447715f, 5.94772f, 0f, 6.5f, 0f)
                curveTo(7.05206f, 0.000263536f, 7.5f, 0.447878f, 7.5f, 1f)
                verticalLineTo(3.0498f)
                curveTo(9.81309f, 3.28411f, 11.7554f, 4.33077f, 12.832f, 5.94531f)
                curveTo(13.1384f, 6.4048f, 13.0142f, 7.02562f, 12.5547f, 7.33203f)
                curveTo(12.0952f, 7.63821f, 11.4743f, 7.51408f, 11.168f, 7.05469f)
                curveTo(10.3971f, 5.89892f, 8.75224f, 5.0001f, 6.5f, 5f)
                curveTo(5.49219f, 5.00001f, 4.28344f, 5.14848f, 3.36621f, 5.5752f)
                curveTo(2.49686f, 5.97967f, 2f, 6.56057f, 2f, 7.5f)
                curveTo(2f, 8.1219f, 2.29454f, 8.70291f, 2.99219f, 9.16797f)
                curveTo(3.71577f, 9.65029f, 4.87836f, 10f, 6.5f, 10f)
                curveTo(8.34201f, 10.0001f, 9.93279f, 10.3206f, 11.0957f, 11.0547f)
                curveTo(12.3172f, 11.8258f, 12.9999f, 13.0166f, 13f, 14.5f)
                curveTo(13.0001f, 15.46f, 12.7143f, 16.2634f, 12.1973f, 16.8975f)
                curveTo(11.6931f, 17.5156f, 11.023f, 17.9072f, 10.3535f, 18.1602f)
                curveTo(9.42593f, 18.5106f, 8.3845f, 18.6338f, 7.5f, 18.6768f)
                verticalLineTo(21f)
                curveTo(7.5f, 21.5523f, 7.05226f, 22f, 6.5f, 22f)
                curveTo(5.94786f, 21.9999f, 5.50004f, 21.5522f, 5.5f, 21f)
                close()
            }
        }
    }.build()
}
