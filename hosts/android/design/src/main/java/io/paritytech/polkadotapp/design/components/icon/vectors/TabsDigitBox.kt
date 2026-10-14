package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.TabsDigitBox: ImageVector
    get() {
        if (_TabsDigitBox != null) {
            return _TabsDigitBox!!
        }
        _TabsDigitBox = ImageVector.Builder(
            name = "TabsDigitBox",
            defaultWidth = 20.dp,
            defaultHeight = 20.dp,
            viewportWidth = 20f,
            viewportHeight = 20f
        ).apply {
            path(
                stroke = SolidColor(Color(0xFFF4F4F5)),
                strokeLineWidth = 2.60596f
            ) {
                moveTo(3.726f, 1.303f)
                lineTo(15.384f, 1.303f)
                arcTo(2.606f, 2.606f, 0f, isMoreThanHalf = false, isPositiveArc = true, 17.99f, 3.909f)
                lineTo(17.99f, 15.567f)
                arcTo(2.606f, 2.606f, 0f, isMoreThanHalf = false, isPositiveArc = true, 15.384f, 18.173f)
                lineTo(3.726f, 18.173f)
                arcTo(2.606f, 2.606f, 0f, isMoreThanHalf = false, isPositiveArc = true, 1.12f, 15.567f)
                lineTo(1.12f, 3.909f)
                arcTo(2.606f, 2.606f, 0f, isMoreThanHalf = false, isPositiveArc = true, 3.726f, 1.303f)
                close()
            }
        }.build()

        return _TabsDigitBox!!
    }

@Suppress("ObjectPropertyName")
private var _TabsDigitBox: ImageVector? = null
