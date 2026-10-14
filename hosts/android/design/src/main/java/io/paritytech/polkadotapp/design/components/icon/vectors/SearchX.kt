package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.SearchX: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "SearchX",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        group(scaleX = 0.375f, scaleY = 0.375f, translationX = 2f, translationY = 2f) {
            path(fill = SolidColor(Color(0xff000000))) {
                moveTo(42.6667f, 24f)
                curveTo(42.6667f, 13.6907f, 34.3093f, 5.3333f, 24f, 5.3333f)
                curveTo(13.6907f, 5.3333f, 5.3333f, 13.6907f, 5.3333f, 24f)
                curveTo(5.3333f, 34.3093f, 13.6907f, 42.6667f, 24f, 42.6667f)
                curveTo(34.3093f, 42.6667f, 42.6667f, 34.3093f, 42.6667f, 24f)
                close()
                moveTo(48f, 24f)
                curveTo(48f, 29.6666f, 46.0316f, 34.8708f, 42.7474f, 38.9766f)
                lineTo(52.5521f, 48.7813f)
                curveTo(53.5934f, 49.8226f, 53.5932f, 51.5107f, 52.5521f, 52.5521f)
                curveTo(51.5107f, 53.5935f, 49.8227f, 53.5935f, 48.7813f, 52.5521f)
                lineTo(38.9766f, 42.7474f)
                curveTo(34.8708f, 46.0316f, 29.6666f, 48f, 24f, 48f)
                curveTo(10.7452f, 48f, 0f, 37.2548f, 0f, 24f)
                curveTo(0f, 10.7452f, 10.7452f, 0f, 24f, 0f)
                curveTo(37.2548f, 0f, 48f, 10.7452f, 48f, 24f)
                close()
            }
        }

        group(scaleX = 0.375f, scaleY = 0.375f, translationX = 7.351f, translationY = 7.349f) {
            path(fill = SolidColor(Color(0xff000000))) {
                moveTo(4.1312f, 0.7039f)
                curveTo(3.1861f, -0.2346f, 1.6539f, -0.2346f, 0.7088f, 0.7039f)
                curveTo(-0.2363f, 1.6425f, -0.2363f, 3.1642f, 0.7088f, 4.1028f)
                lineTo(6.2576f, 9.6133f)
                lineTo(0.7088f, 15.1239f)
                curveTo(-0.2363f, 16.0625f, -0.2363f, 17.5842f, 0.7088f, 18.5227f)
                curveTo(1.6539f, 19.4613f, 3.1861f, 19.4613f, 4.1312f, 18.5227f)
                lineTo(9.68f, 13.0122f)
                lineTo(15.2288f, 18.5227f)
                curveTo(16.1739f, 19.4613f, 17.7061f, 19.4613f, 18.6512f, 18.5227f)
                curveTo(19.5963f, 17.5842f, 19.5963f, 16.0625f, 18.6512f, 15.1239f)
                lineTo(13.1024f, 9.6133f)
                lineTo(18.6512f, 4.1028f)
                curveTo(19.5963f, 3.1642f, 19.5963f, 1.6425f, 18.6512f, 0.7039f)
                curveTo(17.7061f, -0.2346f, 16.1739f, -0.2346f, 15.2288f, 0.7039f)
                lineTo(9.68f, 6.2145f)
                lineTo(4.1312f, 0.7039f)
                close()
            }
        }
    }.build()
}
