package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val ExternalLink: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ExternalLink",
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
            moveTo(17.5f, 7.5f)
            verticalLineTo(2.5f)
            horizontalLineTo(12.5f)
            moveTo(17.5f, 2.5f)
            lineTo(8.333f, 11.667f)
            moveTo(15f, 10.833f)
            verticalLineTo(15.833f)
            curveTo(15f, 16.275f, 14.824f, 16.699f, 14.512f, 17.012f)
            curveTo(14.199f, 17.324f, 13.775f, 17.5f, 13.333f, 17.5f)
            horizontalLineTo(4.167f)
            curveTo(3.725f, 17.5f, 3.301f, 17.324f, 2.988f, 17.012f)
            curveTo(2.676f, 16.699f, 2.5f, 16.275f, 2.5f, 15.833f)
            verticalLineTo(6.667f)
            curveTo(2.5f, 6.225f, 2.676f, 5.801f, 2.988f, 5.488f)
            curveTo(3.301f, 5.176f, 3.725f, 5f, 4.167f, 5f)
            horizontalLineTo(9.167f)
        }
    }.build()
}
