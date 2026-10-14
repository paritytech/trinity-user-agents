package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.Euro: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Euro",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 16f,
        viewportHeight = 16f
    ).apply {
        path(fill = SolidColor(Color(0xFF0052B4))) {
            moveTo(8f, 16f)
            curveTo(12.418f, 16f, 16f, 12.418f, 16f, 8f)
            curveTo(16f, 3.582f, 12.418f, 0f, 8f, 0f)
            curveTo(3.582f, 0f, 0f, 3.582f, 0f, 8f)
            curveTo(0f, 12.418f, 3.582f, 16f, 8f, 16f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(8f, 3.13f)
            lineTo(8.259f, 3.928f)
            horizontalLineTo(9.097f)
            lineTo(8.419f, 4.42f)
            lineTo(8.678f, 5.217f)
            lineTo(8f, 4.725f)
            lineTo(7.322f, 5.217f)
            lineTo(7.581f, 4.42f)
            lineTo(6.903f, 3.928f)
            horizontalLineTo(7.741f)
            lineTo(8f, 3.13f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(4.557f, 4.557f)
            lineTo(5.303f, 4.937f)
            lineTo(5.896f, 4.345f)
            lineTo(5.765f, 5.172f)
            lineTo(6.512f, 5.553f)
            lineTo(5.684f, 5.684f)
            lineTo(5.553f, 6.512f)
            lineTo(5.172f, 5.765f)
            lineTo(4.344f, 5.896f)
            lineTo(4.937f, 5.304f)
            lineTo(4.557f, 4.557f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(3.13f, 8f)
            lineTo(3.928f, 7.741f)
            verticalLineTo(6.903f)
            lineTo(4.42f, 7.581f)
            lineTo(5.217f, 7.322f)
            lineTo(4.725f, 8f)
            lineTo(5.217f, 8.678f)
            lineTo(4.42f, 8.419f)
            lineTo(3.928f, 9.097f)
            verticalLineTo(8.259f)
            lineTo(3.13f, 8f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(4.557f, 11.443f)
            lineTo(4.937f, 10.696f)
            lineTo(4.344f, 10.104f)
            lineTo(5.172f, 10.235f)
            lineTo(5.553f, 9.488f)
            lineTo(5.684f, 10.316f)
            lineTo(6.512f, 10.447f)
            lineTo(5.765f, 10.828f)
            lineTo(5.896f, 11.655f)
            lineTo(5.303f, 11.063f)
            lineTo(4.557f, 11.443f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(8f, 12.87f)
            lineTo(7.741f, 12.072f)
            horizontalLineTo(6.903f)
            lineTo(7.581f, 11.58f)
            lineTo(7.322f, 10.783f)
            lineTo(8f, 11.275f)
            lineTo(8.678f, 10.783f)
            lineTo(8.419f, 11.58f)
            lineTo(9.097f, 12.072f)
            horizontalLineTo(8.259f)
            lineTo(8f, 12.87f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(11.443f, 11.443f)
            lineTo(10.696f, 11.063f)
            lineTo(10.104f, 11.656f)
            lineTo(10.235f, 10.828f)
            lineTo(9.488f, 10.447f)
            lineTo(10.316f, 10.316f)
            lineTo(10.447f, 9.488f)
            lineTo(10.828f, 10.235f)
            lineTo(11.655f, 10.104f)
            lineTo(11.063f, 10.696f)
            lineTo(11.443f, 11.443f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(12.869f, 8f)
            lineTo(12.072f, 8.259f)
            verticalLineTo(9.097f)
            lineTo(11.58f, 8.419f)
            lineTo(10.783f, 8.678f)
            lineTo(11.275f, 8f)
            lineTo(10.783f, 7.322f)
            lineTo(11.58f, 7.581f)
            lineTo(12.072f, 6.903f)
            verticalLineTo(7.741f)
            lineTo(12.869f, 8f)
            close()
        }

        path(fill = SolidColor(Color(0xFFFFDA44))) {
            moveTo(11.443f, 4.557f)
            lineTo(11.063f, 5.304f)
            lineTo(11.655f, 5.896f)
            lineTo(10.827f, 5.765f)
            lineTo(10.447f, 6.512f)
            lineTo(10.316f, 5.684f)
            lineTo(9.488f, 5.553f)
            lineTo(10.235f, 5.172f)
            lineTo(10.104f, 4.345f)
            lineTo(10.696f, 4.937f)
            lineTo(11.443f, 4.557f)
            close()
        }
    }.build()
}
