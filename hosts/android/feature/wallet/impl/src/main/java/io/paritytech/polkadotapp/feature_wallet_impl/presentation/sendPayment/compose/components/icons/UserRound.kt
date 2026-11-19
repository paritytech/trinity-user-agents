package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components.icons

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp

val UserRound: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "UserRound",
        defaultWidth = 20.dp,
        defaultHeight = 20.dp,
        viewportWidth = 20f,
        viewportHeight = 20f
    ).apply {
        path(
            stroke = SolidColor(Color(0xFFF4F4F5)),
            strokeLineWidth = 2f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round
        ) {
            moveTo(10f, 10.8333f)
            curveTo(12.3012f, 10.8333f, 14.1667f, 8.96785f, 14.1667f, 6.66667f)
            curveTo(14.1667f, 4.36548f, 12.3012f, 2.5f, 10f, 2.5f)
            curveTo(7.69881f, 2.5f, 5.83333f, 4.36548f, 5.83333f, 6.66667f)
            curveTo(5.83333f, 8.96785f, 7.69881f, 10.8333f, 10f, 10.8333f)
            close()
            moveTo(10f, 10.8333f)
            curveTo(11.7681f, 10.8333f, 13.4638f, 11.5357f, 14.714f, 12.786f)
            curveTo(15.9643f, 14.0362f, 16.6667f, 15.7319f, 16.6667f, 17.5f)
            moveTo(10f, 10.8333f)
            curveTo(8.23189f, 10.8333f, 6.5362f, 11.5357f, 5.28595f, 12.786f)
            curveTo(4.03571f, 14.0362f, 3.33333f, 15.7319f, 3.33333f, 17.5f)
        }
    }.build()
}
