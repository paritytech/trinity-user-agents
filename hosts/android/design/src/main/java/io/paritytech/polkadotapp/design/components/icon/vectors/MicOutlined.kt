package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.MicOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MicOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 960f,
        viewportHeight = 960f
    ).apply {
        path(fill = SolidColor(Color(0xFFE3E3E3))) {
            moveTo(395f, 525f)
            quadToRelative(-35f, -35f, -35f, -85f)
            verticalLineToRelative(-240f)
            quadToRelative(0f, -50f, 35f, -85f)
            reflectiveQuadToRelative(85f, -35f)
            quadToRelative(50f, 0f, 85f, 35f)
            reflectiveQuadToRelative(35f, 85f)
            verticalLineToRelative(240f)
            quadToRelative(0f, 50f, -35f, 85f)
            reflectiveQuadToRelative(-85f, 35f)
            quadToRelative(-50f, 0f, -85f, -35f)
            close()
            moveTo(480f, 320f)
            close()
            moveTo(440f, 840f)
            verticalLineToRelative(-123f)
            quadToRelative(-104f, -14f, -172f, -93f)
            reflectiveQuadToRelative(-68f, -184f)
            horizontalLineToRelative(80f)
            quadToRelative(0f, 83f, 58.5f, 141.5f)
            reflectiveQuadTo(480f, 640f)
            quadToRelative(83f, 0f, 141.5f, -58.5f)
            reflectiveQuadTo(680f, 440f)
            horizontalLineToRelative(80f)
            quadToRelative(0f, 105f, -68f, 184f)
            reflectiveQuadToRelative(-172f, 93f)
            verticalLineToRelative(123f)
            horizontalLineToRelative(-80f)
            close()
            moveTo(508.5f, 468.5f)
            quadTo(520f, 457f, 520f, 440f)
            verticalLineToRelative(-240f)
            quadToRelative(0f, -17f, -11.5f, -28.5f)
            reflectiveQuadTo(480f, 160f)
            quadToRelative(-17f, 0f, -28.5f, 11.5f)
            reflectiveQuadTo(440f, 200f)
            verticalLineToRelative(240f)
            quadToRelative(0f, 17f, 11.5f, 28.5f)
            reflectiveQuadTo(480f, 480f)
            quadToRelative(17f, 0f, 28.5f, -11.5f)
            close()
        }
    }.build()
}
