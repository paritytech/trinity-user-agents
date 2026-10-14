package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.CloudOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "CloudOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFF080808))) {
            moveTo(1.25f, 15f)
            curveTo(1.25f, 12.858f, 2.474f, 11.003f, 4.26f, 10.095f)
            curveTo(4.254f, 9.981f, 4.25f, 9.866f, 4.25f, 9.75f)
            curveTo(4.25f, 6.298f, 7.048f, 3.5f, 10.5f, 3.5f)
            curveTo(13.067f, 3.5f, 15.27f, 5.048f, 16.232f, 7.26f)
            curveTo(16.321f, 7.254f, 16.41f, 7.25f, 16.5f, 7.25f)
            curveTo(18.709f, 7.25f, 20.5f, 9.041f, 20.5f, 11.25f)
            curveTo(20.5f, 11.401f, 20.488f, 11.55f, 20.472f, 11.697f)
            curveTo(21.836f, 12.531f, 22.75f, 14.032f, 22.75f, 15.75f)
            curveTo(22.75f, 18.373f, 20.623f, 20.5f, 18f, 20.5f)
            horizontalLineTo(6.75f)
            curveTo(3.712f, 20.5f, 1.25f, 18.038f, 1.25f, 15f)
            close()
            moveTo(6.25f, 9.75f)
            curveTo(6.25f, 10.021f, 6.275f, 10.286f, 6.323f, 10.541f)
            curveTo(6.416f, 11.033f, 6.129f, 11.518f, 5.653f, 11.675f)
            curveTo(4.256f, 12.135f, 3.25f, 13.451f, 3.25f, 15f)
            curveTo(3.25f, 16.933f, 4.817f, 18.5f, 6.75f, 18.5f)
            horizontalLineTo(18f)
            curveTo(19.519f, 18.5f, 20.75f, 17.269f, 20.75f, 15.75f)
            curveTo(20.75f, 14.577f, 20.015f, 13.572f, 18.977f, 13.178f)
            curveTo(18.47f, 12.985f, 18.208f, 12.424f, 18.388f, 11.912f)
            curveTo(18.46f, 11.706f, 18.5f, 11.484f, 18.5f, 11.25f)
            curveTo(18.5f, 10.145f, 17.605f, 9.25f, 16.5f, 9.25f)
            curveTo(16.283f, 9.25f, 16.075f, 9.284f, 15.882f, 9.347f)
            curveTo(15.621f, 9.431f, 15.337f, 9.405f, 15.096f, 9.274f)
            curveTo(14.855f, 9.143f, 14.678f, 8.918f, 14.607f, 8.653f)
            curveTo(14.124f, 6.837f, 12.467f, 5.5f, 10.5f, 5.5f)
            curveTo(8.153f, 5.5f, 6.25f, 7.403f, 6.25f, 9.75f)
            close()
        }
    }.build()
}
