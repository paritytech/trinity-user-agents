package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ChatPlaceholder: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ChatPlaceholder",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(12f, 12f)
            moveToRelative(-12f, 0f)
            arcToRelative(12f, 12f, 0f, isMoreThanHalf = true, isPositiveArc = true, 24f, 0f)
            arcToRelative(12f, 12f, 0f, isMoreThanHalf = true, isPositiveArc = true, -24f, 0f)
        }
    }.build()
}
