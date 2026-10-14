package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.WiFi: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "WiFi",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 960f,
        viewportHeight = 960f
    ).apply {
        path(fill = SolidColor(Color(0xFFE3E3E3))) {
            moveToRelative(298f, 651f)
            lineToRelative(-70f, -71f)
            quadToRelative(51f, -48f, 116f, -74f)
            reflectiveQuadToRelative(136f, -26f)
            quadToRelative(71f, 0f, 136f, 26f)
            reflectiveQuadToRelative(116f, 74f)
            lineToRelative(-70f, 71f)
            quadToRelative(-38f, -35f, -84.5f, -53f)
            reflectiveQuadTo(480f, 580f)
            quadToRelative(-51f, 0f, -97.5f, 18f)
            reflectiveQuadTo(298f, 651f)
            close()
            moveTo(73f, 424f)
            lineTo(2f, 353f)
            quadToRelative(97f, -94f, 220.5f, -143.5f)
            reflectiveQuadTo(480f, 160f)
            quadToRelative(134f, 0f, 257.5f, 49.5f)
            reflectiveQuadTo(958f, 353f)
            lineToRelative(-71f, 71f)
            quadToRelative(-82f, -79f, -187f, -121.5f)
            reflectiveQuadTo(480f, 260f)
            quadToRelative(-115f, 0f, -220f, 42.5f)
            reflectiveQuadTo(73f, 424f)
            close()
            moveTo(186f, 538f)
            lineTo(116f, 467f)
            quadToRelative(74f, -71f, 168f, -109f)
            reflectiveQuadToRelative(197f, -38f)
            quadToRelative(103f, 0f, 196.5f, 37.5f)
            reflectiveQuadTo(845f, 466f)
            lineToRelative(-70f, 71f)
            quadToRelative(-60f, -57f, -136.5f, -87f)
            reflectiveQuadTo(480f, 420f)
            quadToRelative(-83f, 0f, -158.5f, 30.5f)
            reflectiveQuadTo(186f, 538f)
            close()
            moveTo(423.5f, 776.5f)
            quadTo(400f, 753f, 400f, 720f)
            reflectiveQuadToRelative(23.5f, -56.5f)
            quadTo(447f, 640f, 480f, 640f)
            reflectiveQuadToRelative(56.5f, 23.5f)
            quadTo(560f, 687f, 560f, 720f)
            reflectiveQuadToRelative(-23.5f, 56.5f)
            quadTo(513f, 800f, 480f, 800f)
            reflectiveQuadToRelative(-56.5f, -23.5f)
            close()
        }
    }.build()
}
