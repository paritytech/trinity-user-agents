package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Failure: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "GraphicElementError",
        defaultWidth = 80.dp,
        defaultHeight = 80.dp,
        viewportWidth = 80f,
        viewportHeight = 80f
    ).apply {
        path(fill = SolidColor(Color(0xFFFF3123))) {
            moveTo(40f, 0f)
            lineTo(40f, 0f)
            arcTo(40f, 40f, 0f, isMoreThanHalf = false, isPositiveArc = true, 80f, 40f)
            lineTo(80f, 40f)
            arcTo(40f, 40f, 0f, isMoreThanHalf = false, isPositiveArc = true, 40f, 80f)
            lineTo(40f, 80f)
            arcTo(40f, 40f, 0f, isMoreThanHalf = false, isPositiveArc = true, 0f, 40f)
            lineTo(0f, 40f)
            arcTo(40f, 40f, 0f, isMoreThanHalf = false, isPositiveArc = true, 40f, 0f)
            close()
        }
        path(fill = SolidColor(Color.White)) {
            moveTo(48.562f, 27.348f)
            curveTo(50.026f, 25.883f, 52.401f, 25.883f, 53.865f, 27.348f)
            curveTo(55.33f, 28.812f, 55.33f, 31.187f, 53.865f, 32.651f)
            lineTo(45.91f, 40.605f)
            lineTo(53.865f, 48.562f)
            curveTo(55.33f, 50.026f, 55.329f, 52.4f, 53.865f, 53.864f)
            curveTo(52.401f, 55.329f, 50.026f, 55.329f, 48.562f, 53.864f)
            lineTo(40.606f, 45.909f)
            lineTo(32.651f, 53.864f)
            curveTo(31.187f, 55.329f, 28.813f, 55.329f, 27.349f, 53.864f)
            curveTo(25.884f, 52.4f, 25.884f, 50.026f, 27.349f, 48.562f)
            lineTo(35.303f, 40.605f)
            lineTo(27.349f, 32.651f)
            curveTo(25.884f, 31.187f, 25.884f, 28.812f, 27.349f, 27.348f)
            curveTo(28.813f, 25.883f, 31.188f, 25.883f, 32.652f, 27.348f)
            lineTo(40.606f, 35.302f)
            lineTo(48.562f, 27.348f)
            close()
        }
    }.build()
}
