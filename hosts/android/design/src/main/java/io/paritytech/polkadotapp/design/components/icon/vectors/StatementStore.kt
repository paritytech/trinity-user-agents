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

val NovaIcons.StatementStore: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "StatementStore",
        defaultWidth = 10.dp,
        defaultHeight = 10.dp,
        viewportWidth = 10f,
        viewportHeight = 10f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(8.43f, 9.289f)
            horizontalLineTo(1.57f)
            verticalLineTo(3.071f)
            lineTo(3.932f, 0.711f)
            horizontalLineTo(8.43f)
            verticalLineTo(9.289f)
            close()
            moveTo(3.496f, 7.386f)
            horizontalLineTo(5.975f)
            verticalLineTo(6.526f)
            horizontalLineTo(3.496f)
            verticalLineTo(7.386f)
            close()
            moveTo(3.496f, 5.729f)
            horizontalLineTo(6.822f)
            verticalLineTo(4.87f)
            horizontalLineTo(3.496f)
            verticalLineTo(5.729f)
            close()
            moveTo(3.496f, 3.214f)
            verticalLineTo(4.073f)
            horizontalLineTo(6.822f)
            verticalLineTo(3.214f)
            horizontalLineTo(3.496f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun StatementStorePreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.StatementStore, contentDescription = null)
    }
}
