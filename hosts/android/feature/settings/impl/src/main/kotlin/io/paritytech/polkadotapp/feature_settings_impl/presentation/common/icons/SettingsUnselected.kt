package io.paritytech.polkadotapp.feature_settings_impl.presentation.common.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val SettingsUnselected: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "SettingsUnselected",
        defaultWidth = 18.dp,
        defaultHeight = 18.dp,
        viewportWidth = 18f,
        viewportHeight = 18f
    ).apply {
        group(
            clipPathData = PathData {
                moveTo(0f, 0f)
                horizontalLineToRelative(18f)
                verticalLineToRelative(18f)
                horizontalLineToRelative(-18f)
                close()
            }
        ) {
            path(
                stroke = SolidColor(Color(0xFF11041F)),
                strokeLineWidth = 1f
            ) {
                moveTo(9f, 9f)
                moveToRelative(-8.5f, 0f)
                arcToRelative(8.5f, 8.5f, 0f, isMoreThanHalf = true, isPositiveArc = true, 17f, 0f)
                arcToRelative(8.5f, 8.5f, 0f, isMoreThanHalf = true, isPositiveArc = true, -17f, 0f)
            }
        }
    }.build()
}
