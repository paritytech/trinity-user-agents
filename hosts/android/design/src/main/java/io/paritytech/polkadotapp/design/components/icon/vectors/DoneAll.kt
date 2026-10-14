package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.DoneAll: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "DoneAll",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(17.965f, 6.705f)
            lineTo(16.555f, 5.295f)
            lineTo(10.215f, 11.635f)
            lineTo(11.625f, 13.045f)
            lineTo(17.965f, 6.705f)
            close()
            moveTo(22.205f, 5.295f)
            lineTo(11.625f, 15.875f)
            lineTo(7.445f, 11.705f)
            lineTo(6.035f, 13.115f)
            lineTo(11.625f, 18.705f)
            lineTo(23.625f, 6.705f)
            lineTo(22.205f, 5.295f)
            close()
            moveTo(0.375f, 13.115f)
            lineTo(5.965f, 18.705f)
            lineTo(7.375f, 17.295f)
            lineTo(1.795f, 11.705f)
            lineTo(0.375f, 13.115f)
            close()
        }
    }.build()
}
