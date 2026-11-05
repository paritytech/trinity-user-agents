package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ShieldBroken: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ShieldBroken",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Same rim as `ShieldOutlined`, so the broken variant reads as the same shield with a crack.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 2f)
            lineTo(20f, 5f)
            verticalLineTo(11.5f)
            curveTo(20f, 16.5f, 16.6f, 20.9f, 12f, 22f)
            curveTo(7.4f, 20.9f, 4f, 16.5f, 4f, 11.5f)
            verticalLineTo(5f)
            close()

            moveTo(12f, 3.8f)
            lineTo(18.6f, 6.3f)
            verticalLineTo(11.6f)
            curveTo(18.6f, 15.7f, 15.8f, 19.3f, 12f, 20.2f)
            curveTo(8.2f, 19.3f, 5.4f, 15.7f, 5.4f, 11.6f)
            verticalLineTo(6.3f)
            close()
        }
        path(
            stroke = SolidColor(Color(0xFF000000)),
            strokeLineWidth = 1.4f,
            strokeLineCap = StrokeCap.Round,
            strokeLineJoin = StrokeJoin.Round,
        ) {
            moveTo(12.4f, 4.6f)
            lineTo(10.6f, 9.4f)
            lineTo(13.4f, 12f)
            lineTo(11.2f, 15.6f)
            lineTo(12.2f, 19.2f)
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun ShieldBrokenPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.ShieldBroken, contentDescription = null)
    }
}
