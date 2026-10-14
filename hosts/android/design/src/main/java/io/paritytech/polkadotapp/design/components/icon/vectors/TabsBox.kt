package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.TabsBox: ImageVector
    get() {
        if (_TabsBox != null) {
            return _TabsBox!!
        }
        _TabsBox = ImageVector.Builder(
            name = "TabsBox",
            defaultWidth = 20.dp,
            defaultHeight = 18.dp,
            viewportWidth = 20f,
            viewportHeight = 18f
        ).apply {
            path(
                stroke = SolidColor(Color(0xFFF4F4F5)),
                strokeLineWidth = 2.60596f,
                strokeLineCap = StrokeCap.Round
            ) {
                moveTo(17.82f, 16.584f)
                verticalLineTo(7.383f)
                curveTo(17.82f, 4.025f, 15.097f, 1.303f, 11.739f, 1.303f)
                horizontalLineTo(1.303f)
            }
        }.build()

        return _TabsBox!!
    }

@Suppress("ObjectPropertyName")
private var _TabsBox: ImageVector? = null
