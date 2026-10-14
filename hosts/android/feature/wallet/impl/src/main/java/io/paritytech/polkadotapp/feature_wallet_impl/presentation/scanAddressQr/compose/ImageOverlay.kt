package io.paritytech.polkadotapp.feature_wallet_impl.presentation.scanAddressQr.compose

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val ImageOverlay: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ImageOverlay",
        defaultWidth = 390.dp,
        defaultHeight = 844.dp,
        viewportWidth = 390f,
        viewportHeight = 844f
    ).apply {
        path(
            fill = Brush.radialGradient(
                colorStops = arrayOf(
                    0f to Color(0x00000000),
                    1f to Color(0xFF000000)
                ),
                center = Offset(195f, 422f),
                radius = 375f
            ),
            pathFillType = PathFillType.EvenOdd
        ) {
            moveTo(390f, 0f)
            horizontalLineTo(0f)
            verticalLineTo(844f)
            horizontalLineTo(390f)
            verticalLineTo(0f)
            close()
            moveTo(63f, 266f)
            curveTo(49.75f, 266f, 39f, 276.74f, 39f, 290f)
            verticalLineTo(554f)
            curveTo(39f, 567.26f, 49.75f, 578f, 63f, 578f)
            horizontalLineTo(327f)
            curveTo(340.26f, 578f, 351f, 567.26f, 351f, 554f)
            verticalLineTo(290f)
            curveTo(351f, 276.74f, 340.26f, 266f, 327f, 266f)
            horizontalLineTo(63f)
            close()
        }
    }.build()
}
