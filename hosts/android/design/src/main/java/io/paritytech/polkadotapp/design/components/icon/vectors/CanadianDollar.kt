package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CanadianDollar: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CanadianDollar",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(fill = SolidColor(Color(0xFFF0F0F0))) {
            moveTo(8f, 16f)
            curveTo(12.418f, 16f, 16f, 12.418f, 16f, 8f)
            curveTo(16f, 3.582f, 12.418f, 0f, 8f, 0f)
            curveTo(3.582f, 0f, 0f, 3.582f, 0f, 8f)
            curveTo(0f, 12.418f, 3.582f, 16f, 8f, 16f)
            close()
        }

        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(16f, 8f)
            curveTo(16f, 4.828f, 14.154f, 2.088f, 11.478f, 0.794f)
            verticalLineTo(15.206f)
            curveTo(14.154f, 13.912f, 16f, 11.172f, 16f, 8f)
            close()
        }

        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(0f, 8f)
            curveTo(0f, 11.172f, 1.846f, 13.912f, 4.522f, 15.206f)
            verticalLineTo(0.794f)
            curveTo(1.846f, 2.088f, 0f, 4.828f, 0f, 8f)
            close()
        }

        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(9.391f, 9.043f)
            lineTo(10.783f, 8.348f)
            lineTo(10.087f, 8f)
            verticalLineTo(7.304f)
            lineTo(8.696f, 8f)
            lineTo(9.391f, 6.609f)
            horizontalLineTo(8.696f)
            lineTo(8f, 5.565f)
            lineTo(7.304f, 6.609f)
            horizontalLineTo(6.609f)
            lineTo(7.304f, 8f)
            lineTo(5.913f, 7.304f)
            verticalLineTo(8f)
            lineTo(5.217f, 8.348f)
            lineTo(6.609f, 9.043f)
            lineTo(6.261f, 9.739f)
            horizontalLineTo(7.652f)
            verticalLineTo(10.783f)
            horizontalLineTo(8.348f)
            verticalLineTo(9.739f)
            horizontalLineTo(9.739f)
            lineTo(9.391f, 9.043f)
            close()
        }
    }.build()
}
