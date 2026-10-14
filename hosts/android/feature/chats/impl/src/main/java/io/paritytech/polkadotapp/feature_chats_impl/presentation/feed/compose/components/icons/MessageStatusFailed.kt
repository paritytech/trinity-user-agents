package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val MessageStatusFailed: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MessageStatusFailed",
        defaultWidth = 12.dp,
        defaultHeight = 12.dp,
        viewportWidth = 12f,
        viewportHeight = 12f
    ).apply {
        path(
            stroke = SolidColor(Color(0xFF6F727A)),
            strokeLineWidth = 1f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            // Circle outline centered at (6, 6) with radius 4.5
            moveTo(1.5f, 6f)
            arcToRelative(4.5f, 4.5f, 0f, true, true, 9f, 0f)
            arcToRelative(4.5f, 4.5f, 0f, true, true, -9f, 0f)
            // Cross
            moveTo(4.5f, 4.5f)
            lineTo(7.5f, 7.5f)
            moveTo(7.5f, 4.5f)
            lineTo(4.5f, 7.5f)
        }
    }.build()
}
