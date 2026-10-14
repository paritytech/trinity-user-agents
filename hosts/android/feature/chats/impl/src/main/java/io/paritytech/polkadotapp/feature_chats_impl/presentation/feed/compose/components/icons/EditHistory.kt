package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val EditHistory: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "EditHistory",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 17f,
        viewportHeight = 17f
    ).apply {
        path(fill = SolidColor(Color(0xFF1C1B1F))) {
            moveTo(8.292f, 16.667f)
            curveTo(6.139f, 16.667f, 4.271f, 15.951f, 2.688f, 14.521f)
            curveTo(1.104f, 13.09f, 0.208f, 11.306f, 0f, 9.167f)
            horizontalLineTo(1.688f)
            curveTo(1.896f, 10.847f, 2.628f, 12.24f, 3.885f, 13.344f)
            curveTo(5.142f, 14.448f, 6.611f, 15f, 8.292f, 15f)
            curveTo(10.153f, 15f, 11.729f, 14.354f, 13.021f, 13.063f)
            curveTo(14.313f, 11.771f, 14.958f, 10.194f, 14.958f, 8.333f)
            curveTo(14.958f, 6.472f, 14.313f, 4.896f, 13.021f, 3.604f)
            curveTo(11.729f, 2.313f, 10.153f, 1.667f, 8.292f, 1.667f)
            curveTo(7.097f, 1.667f, 5.99f, 1.962f, 4.969f, 2.552f)
            curveTo(3.948f, 3.142f, 3.139f, 3.958f, 2.542f, 5f)
            horizontalLineTo(4.958f)
            verticalLineTo(6.667f)
            horizontalLineTo(0.125f)
            curveTo(0.528f, 4.722f, 1.493f, 3.125f, 3.021f, 1.875f)
            curveTo(4.549f, 0.625f, 6.306f, 0f, 8.292f, 0f)
            curveTo(9.444f, 0f, 10.528f, 0.219f, 11.542f, 0.656f)
            curveTo(12.556f, 1.094f, 13.438f, 1.688f, 14.188f, 2.438f)
            curveTo(14.938f, 3.188f, 15.531f, 4.069f, 15.969f, 5.083f)
            curveTo(16.406f, 6.097f, 16.625f, 7.181f, 16.625f, 8.333f)
            curveTo(16.625f, 9.486f, 16.406f, 10.569f, 15.969f, 11.583f)
            curveTo(15.531f, 12.597f, 14.938f, 13.479f, 14.188f, 14.229f)
            curveTo(13.438f, 14.979f, 12.556f, 15.573f, 11.542f, 16.01f)
            curveTo(10.528f, 16.448f, 9.444f, 16.667f, 8.292f, 16.667f)
            close()

            moveTo(10.625f, 11.833f)
            lineTo(7.458f, 8.667f)
            verticalLineTo(4.167f)
            horizontalLineTo(9.125f)
            verticalLineTo(8f)
            lineTo(11.792f, 10.667f)
            lineTo(10.625f, 11.833f)
            close()
        }
    }.build()
}
