package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

/** Value leaving a card: an arrow pointing out of its lower-left corner. */
val NovaIcons.CardSent: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CardSent",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(
            stroke = SolidColor(Color(0xFFECECEC)),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            moveTo(2f, 12f)
            verticalLineTo(7f)
            curveTo(2f, 5.89543f, 2.89543f, 5f, 4f, 5f)
            horizontalLineTo(20f)
            curveTo(21.1046f, 5f, 22f, 5.89543f, 22f, 7f)
            verticalLineTo(17f)
            curveTo(22f, 18.1046f, 21.1046f, 19f, 20f, 19f)
            horizontalLineTo(11f)
            moveTo(6.36351f, 16.1213f)
            lineTo(2.12087f, 20.3639f)
            moveTo(2.12087f, 16.1213f)
            lineTo(2.12087f, 20.3639f)
            lineTo(6.36351f, 20.3639f)
        }
        path(fill = SolidColor(Color(0xFFECECEC))) {
            moveTo(15f, 12f)
            arcTo(3f, 3f, 0f, isMoreThanHalf = true, isPositiveArc = true, 9f, 12f)
            arcTo(3f, 3f, 0f, isMoreThanHalf = true, isPositiveArc = true, 15f, 12f)
            close()
        }
    }.build()
}
