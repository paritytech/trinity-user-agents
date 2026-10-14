package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.PersonAdd: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "PersonAdd",
        defaultWidth = 21.dp,
        defaultHeight = 20.dp,
        viewportWidth = 21f,
        viewportHeight = 20f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(13f, 10f)
            curveTo(14.842f, 10f, 16.333f, 8.508f, 16.333f, 6.667f)
            curveTo(16.333f, 4.825f, 14.842f, 3.333f, 13f, 3.333f)
            curveTo(11.158f, 3.333f, 9.667f, 4.825f, 9.667f, 6.667f)
            curveTo(9.667f, 8.508f, 11.158f, 10f, 13f, 10f)
            close()
            moveTo(13f, 5f)
            curveTo(13.917f, 5f, 14.667f, 5.75f, 14.667f, 6.667f)
            curveTo(14.667f, 7.583f, 13.917f, 8.333f, 13f, 8.333f)
            curveTo(12.083f, 8.333f, 11.333f, 7.583f, 11.333f, 6.667f)
            curveTo(11.333f, 5.75f, 12.083f, 5f, 13f, 5f)
            close()
            moveTo(13f, 11.667f)
            curveTo(10.775f, 11.667f, 6.333f, 12.783f, 6.333f, 15f)
            verticalLineTo(16.667f)
            horizontalLineTo(19.667f)
            verticalLineTo(15f)
            curveTo(19.667f, 12.783f, 15.225f, 11.667f, 13f, 11.667f)
            close()
            moveTo(8f, 15f)
            curveTo(8.183f, 14.4f, 10.758f, 13.333f, 13f, 13.333f)
            curveTo(15.25f, 13.333f, 17.833f, 14.408f, 18f, 15f)
            horizontalLineTo(8f)
            close()
            moveTo(5.5f, 12.5f)
            verticalLineTo(10f)
            horizontalLineTo(8f)
            verticalLineTo(8.333f)
            horizontalLineTo(5.5f)
            verticalLineTo(5.833f)
            horizontalLineTo(3.833f)
            verticalLineTo(8.333f)
            horizontalLineTo(1.333f)
            verticalLineTo(10f)
            horizontalLineTo(3.833f)
            verticalLineTo(12.5f)
            horizontalLineTo(5.5f)
            close()
        }
    }.build()
}
