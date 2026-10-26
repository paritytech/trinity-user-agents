package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ScanFrame: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ScanFrame",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    )
        .addPath(
            pathData = addPathNodes(SCAN_FRAME_PATH),
            stroke = SolidColor(Color.Black),
            strokeLineWidth = 2.2f,
            strokeLineCap = StrokeCap.Round
        )
        .build()
}

private const val SCAN_FRAME_PATH =
    "M1.3 7V4A2.7 2.7 0 0 1 4 1.3h3M17 1.3h3A2.7 2.7 0 0 1 22.7 4v3M22.7 17v3a2.7 2.7 0 0 1-2.7 2.7h-3M7 22.7H4A2.7 2.7 0 0 1 1.3 20v-3"
