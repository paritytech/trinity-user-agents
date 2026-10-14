package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Barcode: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Barcode",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(3f, 11f)
            horizontalLineTo(11f)
            verticalLineTo(3f)
            horizontalLineTo(3f)
            verticalLineTo(11f)
            close()
            moveTo(5f, 5f)
            horizontalLineTo(9f)
            verticalLineTo(9f)
            horizontalLineTo(5f)
            verticalLineTo(5f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(3f, 21f)
            horizontalLineTo(11f)
            verticalLineTo(13f)
            horizontalLineTo(3f)
            verticalLineTo(21f)
            close()
            moveTo(5f, 15f)
            horizontalLineTo(9f)
            verticalLineTo(19f)
            horizontalLineTo(5f)
            verticalLineTo(15f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(13f, 3f)
            verticalLineTo(11f)
            horizontalLineTo(21f)
            verticalLineTo(3f)
            horizontalLineTo(13f)
            close()
            moveTo(19f, 9f)
            horizontalLineTo(15f)
            verticalLineTo(5f)
            horizontalLineTo(19f)
            verticalLineTo(9f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(21f, 19f)
            horizontalLineTo(19f)
            verticalLineTo(21f)
            horizontalLineTo(21f)
            verticalLineTo(19f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(15f, 13f)
            horizontalLineTo(13f)
            verticalLineTo(15f)
            horizontalLineTo(15f)
            verticalLineTo(13f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(17f, 15f)
            horizontalLineTo(15f)
            verticalLineTo(17f)
            horizontalLineTo(17f)
            verticalLineTo(15f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(15f, 17f)
            horizontalLineTo(13f)
            verticalLineTo(19f)
            horizontalLineTo(15f)
            verticalLineTo(17f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(17f, 19f)
            horizontalLineTo(15f)
            verticalLineTo(21f)
            horizontalLineTo(17f)
            verticalLineTo(19f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(19f, 17f)
            horizontalLineTo(17f)
            verticalLineTo(19f)
            horizontalLineTo(19f)
            verticalLineTo(17f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(19f, 13f)
            horizontalLineTo(17f)
            verticalLineTo(15f)
            horizontalLineTo(19f)
            verticalLineTo(13f)
            close()
        }
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(21f, 15f)
            horizontalLineTo(19f)
            verticalLineTo(17f)
            horizontalLineTo(21f)
            verticalLineTo(15f)
            close()
        }
    }.build()
}
