package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val MessageStatusSent: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MessageStatusSent",
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
            moveTo(10f, 3f)
            lineTo(4.5f, 8.5f)
            lineTo(2f, 6f)
        }
    }.build()
}
