package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.HideDocklet: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "HideDocklet",
        defaultWidth = 20.dp,
        defaultHeight = 20.dp,
        viewportWidth = 20f,
        viewportHeight = 20f
    ).apply {
        group(translationX = 1.7002f, translationY = 1.0224f) {
            path(fill = SolidColor(Color(0xFF000000))) {
                moveTo(15f, 14.6544f)
                curveTo(14.9998f, 13.7157f, 14.2386f, 12.9552f, 13.2998f, 12.9552f)
                horizontalLineTo(3.2998f)
                curveTo(2.361f, 12.9552f, 1.5998f, 13.7157f, 1.5996f, 14.6544f)
                curveTo(1.5996f, 15.5933f, 2.3609f, 16.3546f, 3.2998f, 16.3546f)
                horizontalLineTo(13.2998f)
                curveTo(14.2387f, 16.3546f, 15f, 15.5933f, 15f, 14.6544f)
                close()
                moveTo(2.7227f, 9.5694f)
                curveTo(2.7227f, 5.7031f, 5.5022f, 2.4877f, 9.1719f, 1.8087f)
                lineTo(8.6611f, 1.461f)
                curveTo(8.2958f, 1.2125f, 8.2007f, 0.715f, 8.4492f, 0.3497f)
                curveTo(8.6977f, -0.0155f, 9.1953f, -0.1097f, 9.5605f, 0.1388f)
                lineTo(11.7402f, 1.6212f)
                curveTo(12.1001f, 1.8662f, 12.1976f, 2.354f, 11.96f, 2.7188f)
                lineTo(10.4922f, 4.9737f)
                curveTo(10.2511f, 5.3436f, 9.7558f, 5.4479f, 9.3857f, 5.2071f)
                curveTo(9.0155f, 4.966f, 8.9103f, 4.4699f, 9.1514f, 4.0997f)
                lineTo(9.6387f, 3.3497f)
                curveTo(6.6269f, 3.8197f, 4.3223f, 6.4258f, 4.3223f, 9.5694f)
                curveTo(4.322f, 10.011f, 3.9641f, 10.3692f, 3.5225f, 10.3692f)
                curveTo(3.0808f, 10.3692f, 2.7229f, 10.011f, 2.7227f, 9.5694f)
                close()
                moveTo(16.5996f, 14.6544f)
                curveTo(16.5996f, 16.4769f, 15.1223f, 17.9552f, 13.2998f, 17.9552f)
                horizontalLineTo(3.2998f)
                curveTo(1.4773f, 17.9552f, 0f, 16.4769f, 0f, 14.6544f)
                curveTo(0.0002f, 12.832f, 1.4774f, 11.3546f, 3.2998f, 11.3546f)
                horizontalLineTo(13.2998f)
                curveTo(15.1222f, 11.3546f, 16.5994f, 12.832f, 16.5996f, 14.6544f)
                close()
            }
        }
    }.build()
}
