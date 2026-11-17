package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val ChatBubbleOvalLeft: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ChatBubbleOvalLeft",
        defaultWidth = 20.dp,
        defaultHeight = 20.dp,
        viewportWidth = 20f,
        viewportHeight = 20f
    ).apply {
        path(
            stroke = SolidColor(Color.White),
            strokeLineWidth = 1.6f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            moveTo(17.5f, 10f)
            curveTo(17.5f, 13.797f, 14.142f, 16.875f, 10f, 16.875f)
            curveTo(9.26f, 16.875f, 8.546f, 16.777f, 7.871f, 16.594f)
            curveTo(7.058f, 17.165f, 6.068f, 17.5f, 5f, 17.5f)
            curveTo(4.834f, 17.5f, 4.67f, 17.492f, 4.508f, 17.476f)
            curveTo(4.375f, 17.463f, 4.243f, 17.445f, 4.113f, 17.422f)
            curveTo(4.439f, 17.037f, 4.688f, 16.586f, 4.839f, 16.092f)
            curveTo(5.019f, 15.498f, 4.703f, 14.904f, 4.281f, 14.448f)
            curveTo(3.17f, 13.249f, 2.5f, 11.696f, 2.5f, 10f)
            curveTo(2.5f, 6.203f, 5.858f, 3.125f, 10f, 3.125f)
            curveTo(14.142f, 3.125f, 17.5f, 6.203f, 17.5f, 10f)
            close()
        }
    }.build()
}
