package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.WalletFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "WalletFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(2.273f, 5.625f)
            curveTo(3.066f, 4.925f, 4.109f, 4.5f, 5.25f, 4.5f)
            horizontalLineTo(18.75f)
            curveTo(19.891f, 4.5f, 20.933f, 4.925f, 21.727f, 5.625f)
            curveTo(21.542f, 4.145f, 20.28f, 3f, 18.75f, 3f)
            horizontalLineTo(5.25f)
            curveTo(3.72f, 3f, 2.457f, 4.145f, 2.273f, 5.625f)
            close()
        }
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(2.273f, 8.625f)
            curveTo(3.066f, 7.925f, 4.109f, 7.5f, 5.25f, 7.5f)
            horizontalLineTo(18.75f)
            curveTo(19.891f, 7.5f, 20.933f, 7.925f, 21.727f, 8.625f)
            curveTo(21.542f, 7.145f, 20.28f, 6f, 18.75f, 6f)
            horizontalLineTo(5.25f)
            curveTo(3.72f, 6f, 2.457f, 7.145f, 2.273f, 8.625f)
            close()
        }
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(5.25f, 9f)
            curveTo(3.593f, 9f, 2.25f, 10.343f, 2.25f, 12f)
            verticalLineTo(18f)
            curveTo(2.25f, 19.657f, 3.593f, 21f, 5.25f, 21f)
            horizontalLineTo(18.75f)
            curveTo(20.407f, 21f, 21.75f, 19.657f, 21.75f, 18f)
            verticalLineTo(12f)
            curveTo(21.75f, 10.343f, 20.407f, 9f, 18.75f, 9f)
            horizontalLineTo(15f)
            curveTo(14.586f, 9f, 14.25f, 9.336f, 14.25f, 9.75f)
            curveTo(14.25f, 10.993f, 13.243f, 12f, 12f, 12f)
            curveTo(10.757f, 12f, 9.75f, 10.993f, 9.75f, 9.75f)
            curveTo(9.75f, 9.336f, 9.414f, 9f, 9f, 9f)
            horizontalLineTo(5.25f)
            close()
        }
    }.build()
}
