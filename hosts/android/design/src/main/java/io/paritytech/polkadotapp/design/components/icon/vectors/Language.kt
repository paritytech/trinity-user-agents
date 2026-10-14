package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Language: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Language",
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
            moveTo(5f, 8f)
            lineTo(11f, 14f)
            moveTo(4f, 14f)
            lineTo(10f, 8f)
            lineTo(12f, 5f)
            moveTo(2f, 5f)
            horizontalLineTo(14f)
            moveTo(7f, 2f)
            horizontalLineTo(8f)
            moveTo(22f, 22f)
            lineTo(17f, 12f)
            lineTo(12f, 22f)
            moveTo(14f, 18f)
            horizontalLineTo(20f)
        }
    }.build()
}
