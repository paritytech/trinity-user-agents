package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.NFC: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NFC",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 960f,
        viewportHeight = 960f
    ).apply {
        path(fill = SolidColor(Color(0xFFE3E3E3))) {
            moveTo(280f, 680f)
            horizontalLineToRelative(400f)
            verticalLineToRelative(-400f)
            lineTo(520f, 280f)
            quadToRelative(-33f, 0f, -56.5f, 23.5f)
            reflectiveQuadTo(440f, 360f)
            verticalLineToRelative(52f)
            quadToRelative(-20f, 11f, -30f, 28f)
            reflectiveQuadToRelative(-10f, 40f)
            quadToRelative(0f, 33f, 23.5f, 56.5f)
            reflectiveQuadTo(480f, 560f)
            quadToRelative(33f, 0f, 56.5f, -23.5f)
            reflectiveQuadTo(560f, 480f)
            quadToRelative(0f, -23f, -11f, -40f)
            reflectiveQuadToRelative(-29f, -28f)
            verticalLineToRelative(-52f)
            horizontalLineToRelative(80f)
            verticalLineToRelative(240f)
            lineTo(360f, 600f)
            verticalLineToRelative(-240f)
            horizontalLineToRelative(40f)
            verticalLineToRelative(-80f)
            lineTo(280f, 280f)
            verticalLineToRelative(400f)
            close()
            moveTo(200f, 840f)
            quadToRelative(-33f, 0f, -56.5f, -23.5f)
            reflectiveQuadTo(120f, 760f)
            verticalLineToRelative(-560f)
            quadToRelative(0f, -33f, 23.5f, -56.5f)
            reflectiveQuadTo(200f, 120f)
            horizontalLineToRelative(560f)
            quadToRelative(33f, 0f, 56.5f, 23.5f)
            reflectiveQuadTo(840f, 200f)
            verticalLineToRelative(560f)
            quadToRelative(0f, 33f, -23.5f, 56.5f)
            reflectiveQuadTo(760f, 840f)
            lineTo(200f, 840f)
            close()
            moveTo(200f, 760f)
            horizontalLineToRelative(560f)
            verticalLineToRelative(-560f)
            lineTo(200f, 200f)
            verticalLineToRelative(560f)
            close()
            moveTo(200f, 200f)
            verticalLineToRelative(560f)
            verticalLineToRelative(-560f)
            close()
        }
    }.build()
}
