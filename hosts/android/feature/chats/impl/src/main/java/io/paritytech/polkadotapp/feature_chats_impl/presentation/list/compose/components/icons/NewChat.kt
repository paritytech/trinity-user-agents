package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val NewChat: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NewChat",
        defaultWidth = 28.dp,
        defaultHeight = 28.dp,
        viewportWidth = 28f,
        viewportHeight = 28f
    ).apply {
        path(
            stroke = SolidColor(Color(0xFFECECEC)),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            moveTo(5.833f, 14f)
            horizontalLineTo(22.167f)
            moveTo(14f, 5.833f)
            verticalLineTo(22.167f)
        }
    }.build()
}
