package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val MessageStatusPending: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MessageStatusPending",
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
            moveTo(6f, 3f)
            verticalLineTo(6f)
            lineTo(8f, 7f)
            moveTo(11f, 6f)
            curveTo(11f, 8.761f, 8.761f, 11f, 6f, 11f)
            curveTo(3.239f, 11f, 1f, 8.761f, 1f, 6f)
            curveTo(1f, 3.239f, 3.239f, 1f, 6f, 1f)
            curveTo(8.761f, 1f, 11f, 3.239f, 11f, 6f)
            close()
        }
    }.build()
}
