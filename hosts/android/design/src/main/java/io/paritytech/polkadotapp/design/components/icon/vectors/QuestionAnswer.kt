package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.QuestionAnswer: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "QuestionAnswer",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            // Back bubble
            moveTo(15f, 4f)
            verticalLineTo(11f)
            horizontalLineTo(5.17f)
            lineTo(4f, 12.17f)
            verticalLineTo(4f)
            curveTo(4f, 3.45f, 4.45f, 3f, 5f, 3f)
            horizontalLineTo(14f)
            curveTo(14.55f, 3f, 15f, 3.45f, 15f, 4f)
            close()
            // Front bubble
            moveTo(19f, 8f)
            horizontalLineTo(17f)
            verticalLineTo(13f)
            horizontalLineTo(7f)
            verticalLineTo(15f)
            curveTo(7f, 15.55f, 7.45f, 16f, 8f, 16f)
            horizontalLineTo(18.83f)
            lineTo(21f, 18.17f)
            verticalLineTo(9f)
            curveTo(21f, 8.45f, 20.55f, 8f, 20f, 8f)
            horizontalLineTo(19f)
            close()
        }
    }.build()
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun QuestionAnswerPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.QuestionAnswer, contentDescription = null)
    }
}
