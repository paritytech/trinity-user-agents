package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Dollar: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Dollar",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(fill = SolidColor(Color(0xFFF0F0F0))) {
            moveTo(8f, 16f)
            curveTo(12.4183f, 16f, 16f, 12.4183f, 16f, 8f)
            curveTo(16f, 3.58172f, 12.4183f, 0f, 8f, 0f)
            curveTo(3.58172f, 0f, 0f, 3.58172f, 0f, 8f)
            curveTo(0f, 12.4183f, 3.58172f, 16f, 8f, 16f)
            close()
        }

        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(7.652f, 8f)
            horizontalLineTo(16f)
            curveTo(16f, 7.278f, 15.904f, 6.578f, 15.725f, 5.913f)
            horizontalLineTo(7.652f)
            verticalLineTo(8f)
            close()
        }
        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(7.652f, 3.826f)
            horizontalLineTo(14.826f)
            curveTo(14.336f, 3.027f, 13.71f, 2.321f, 12.98f, 1.739f)
            horizontalLineTo(7.652f)
            verticalLineTo(3.826f)
            close()
        }
        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(8f, 16f)
            curveTo(9.883f, 16f, 11.613f, 15.349f, 12.98f, 14.261f)
            horizontalLineTo(3.02f)
            curveTo(4.387f, 15.349f, 6.117f, 16f, 8f, 16f)
            close()
        }
        path(fill = SolidColor(Color(0xFFD80027))) {
            moveTo(1.174f, 12.174f)
            horizontalLineTo(14.826f)
            curveTo(15.219f, 11.532f, 15.524f, 10.831f, 15.724f, 10.087f)
            horizontalLineTo(0.276f)
            curveTo(0.476f, 10.831f, 0.781f, 11.532f, 1.174f, 12.174f)
            close()
        }
        path(fill = SolidColor(Color(0xFF0052B4))) {
            moveTo(3.706f, 1.249f)
            horizontalLineTo(4.435f)
            lineTo(3.757f, 1.742f)
            lineTo(4.016f, 2.539f)
            lineTo(3.338f, 2.046f)
            lineTo(2.659f, 2.539f)
            lineTo(2.883f, 1.85f)
            curveTo(2.286f, 2.348f, 1.763f, 2.931f, 1.332f, 3.58f)
            horizontalLineTo(1.565f)
            lineTo(1.134f, 3.893f)
            curveTo(1.066f, 4.006f, 1.002f, 4.12f, 0.94f, 4.235f)
            lineTo(1.146f, 4.87f)
            lineTo(0.762f, 4.59f)
            curveTo(0.666f, 4.793f, 0.579f, 5f, 0.5f, 5.211f)
            lineTo(0.727f, 5.91f)
            horizontalLineTo(1.565f)
            lineTo(0.887f, 6.403f)
            lineTo(1.146f, 7.2f)
            lineTo(0.468f, 6.707f)
            lineTo(0.062f, 7.002f)
            curveTo(0.021f, 7.329f, 0f, 7.662f, 0f, 8f)
            horizontalLineTo(8f)
            curveTo(8f, 3.582f, 8f, 3.061f, 8f, 0f)
            curveTo(6.42f, 0f, 4.946f, 0.458f, 3.706f, 1.249f)
            close()

            moveTo(4.016f, 7.2f)
            lineTo(3.338f, 6.707f)
            lineTo(2.659f, 7.2f)
            lineTo(2.919f, 6.403f)
            lineTo(2.24f, 5.91f)
            horizontalLineTo(3.079f)
            lineTo(3.338f, 5.113f)
            lineTo(3.597f, 5.91f)
            horizontalLineTo(4.435f)
            lineTo(3.757f, 6.403f)
            lineTo(4.016f, 7.2f)
            close()

            moveTo(3.757f, 4.072f)
            lineTo(4.016f, 4.87f)
            lineTo(3.338f, 4.377f)
            lineTo(2.659f, 4.87f)
            lineTo(2.919f, 4.072f)
            lineTo(2.24f, 3.58f)
            horizontalLineTo(3.079f)
            lineTo(3.338f, 2.783f)
            lineTo(3.597f, 3.58f)
            horizontalLineTo(4.435f)
            lineTo(3.757f, 4.072f)
            close()

            moveTo(6.885f, 7.2f)
            lineTo(6.207f, 6.707f)
            lineTo(5.529f, 7.2f)
            lineTo(5.788f, 6.403f)
            lineTo(5.11f, 5.91f)
            horizontalLineTo(5.948f)
            lineTo(6.207f, 5.113f)
            lineTo(6.466f, 5.91f)
            horizontalLineTo(7.304f)
            lineTo(6.626f, 6.403f)
            lineTo(6.885f, 7.2f)
            close()

            moveTo(6.626f, 4.072f)
            lineTo(6.885f, 4.87f)
            lineTo(6.207f, 4.377f)
            lineTo(5.529f, 4.87f)
            lineTo(5.788f, 4.072f)
            lineTo(5.11f, 3.58f)
            horizontalLineTo(5.948f)
            lineTo(6.207f, 2.783f)
            lineTo(6.466f, 3.58f)
            horizontalLineTo(7.304f)
            lineTo(6.626f, 4.072f)
            close()

            moveTo(6.626f, 1.742f)
            lineTo(6.885f, 2.539f)
            lineTo(6.207f, 2.046f)
            lineTo(5.529f, 2.539f)
            lineTo(5.788f, 1.742f)
            lineTo(5.11f, 1.249f)
            horizontalLineTo(5.948f)
            lineTo(6.207f, 0.452f)
            lineTo(6.466f, 1.249f)
            horizontalLineTo(7.304f)
            lineTo(6.626f, 1.742f)
            close()
        }
    }.build()
}
