package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val RotateCw: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "RotateCw",
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
            curveTo(17.5f, 11.483f, 17.06f, 12.933f, 16.236f, 14.167f)
            curveTo(15.412f, 15.4f, 14.241f, 16.361f, 12.87f, 16.929f)
            curveTo(11.5f, 17.497f, 9.992f, 17.645f, 8.537f, 17.356f)
            curveTo(7.082f, 17.067f, 5.746f, 16.352f, 4.697f, 15.303f)
            curveTo(3.648f, 14.254f, 2.933f, 12.918f, 2.644f, 11.463f)
            curveTo(2.355f, 10.008f, 2.503f, 8.5f, 3.071f, 7.13f)
            curveTo(3.639f, 5.759f, 4.6f, 4.588f, 5.833f, 3.764f)
            curveTo(7.067f, 2.94f, 8.517f, 2.5f, 10f, 2.5f)
            curveTo(12.1f, 2.5f, 14.108f, 3.333f, 15.617f, 4.783f)
            lineTo(17.5f, 6.667f)
            moveTo(13.333f, 6.667f)
            horizontalLineTo(17.5f)
            lineTo(17.5f, 2.5f)
        }
    }.build()
}
