package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ArrowDropdownBig: ImageVector
    get() = arrowDropdownBig

private val arrowDropdownBig: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ArrowDropdownBig",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFF4F4F5))) {
            moveTo(18.707f, 8.293f)
            curveTo(19.097f, 8.684f, 19.097f, 9.317f, 18.707f, 9.707f)
            lineTo(12.707f, 15.707f)
            curveTo(12.316f, 16.098f, 11.683f, 16.098f, 11.293f, 15.707f)
            lineTo(5.293f, 9.707f)
            curveTo(4.902f, 9.317f, 4.902f, 8.684f, 5.293f, 8.293f)
            curveTo(5.683f, 7.903f, 6.316f, 7.903f, 6.707f, 8.293f)
            lineTo(12f, 13.586f)
            lineTo(17.293f, 8.293f)
            curveTo(17.683f, 7.903f, 18.316f, 7.903f, 18.707f, 8.293f)
            close()
        }
    }.build()
}
