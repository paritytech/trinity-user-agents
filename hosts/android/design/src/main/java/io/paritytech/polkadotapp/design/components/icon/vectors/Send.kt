package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Send: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Send",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(3.51f, 6.03f)
            lineTo(11.02f, 9.25f)
            lineTo(3.5f, 8.25f)
            lineTo(3.51f, 6.03f)
            close()
            moveTo(11.01f, 14.75f)
            lineTo(3.5f, 17.97f)
            verticalLineTo(15.75f)
            lineTo(11.01f, 14.75f)
            close()
            moveTo(1.51f, 3f)
            lineTo(1.5f, 10f)
            lineTo(16.5f, 12f)
            lineTo(1.5f, 14f)
            lineTo(1.51f, 21f)
            lineTo(22.5f, 12f)
            lineTo(1.51f, 3f)
            close()
        }
    }.build()
}
