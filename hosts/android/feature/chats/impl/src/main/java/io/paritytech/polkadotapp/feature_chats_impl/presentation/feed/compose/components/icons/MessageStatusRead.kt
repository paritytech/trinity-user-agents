package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val MessageStatusRead: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MessageStatusRead",
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
            moveTo(8.1f, 3f)
            lineTo(2.6f, 8.5f)
            lineTo(0.75f, 6.75f)
            moveTo(11.25f, 3f)
            lineTo(5.75f, 8.5f)
            lineTo(5.25f, 8f)
        }
    }.build()
}
