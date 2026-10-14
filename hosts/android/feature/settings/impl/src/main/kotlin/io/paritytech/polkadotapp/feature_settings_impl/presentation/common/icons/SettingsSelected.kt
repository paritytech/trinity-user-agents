package io.paritytech.polkadotapp.feature_settings_impl.presentation.common.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathData
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val SettingsSelected: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "SettingsSelected",
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
            path(fill = SolidColor(Color(0xFF35C759))) {
                moveTo(9f, 9f)
                moveToRelative(-9f, 0f)
                arcToRelative(9f, 9f, 0f, isMoreThanHalf = true, isPositiveArc = true, 18f, 0f)
                arcToRelative(9f, 9f, 0f, isMoreThanHalf = true, isPositiveArc = true, -18f, 0f)
            }
            path(
                fill = SolidColor(Color.White),
                pathFillType = PathFillType.EvenOdd
            ) {
                moveTo(12.502f, 5.943f)
                curveTo(12.81f, 6.22f, 12.835f, 6.694f, 12.557f, 7.002f)
                lineTo(8.128f, 11.923f)
                curveTo(7.802f, 12.285f, 7.24f, 12.3f, 6.895f, 11.956f)
                lineTo(5.47f, 10.53f)
                curveTo(5.177f, 10.237f, 5.177f, 9.763f, 5.47f, 9.47f)
                curveTo(5.763f, 9.177f, 6.237f, 9.177f, 6.53f, 9.47f)
                lineTo(7.471f, 10.411f)
                lineTo(11.443f, 5.998f)
                curveTo(11.72f, 5.69f, 12.194f, 5.665f, 12.502f, 5.943f)
                close()
            }
        }
    }.build()
}
