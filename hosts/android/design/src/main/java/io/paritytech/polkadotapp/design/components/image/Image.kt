package io.paritytech.polkadotapp.design.components.image

import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.DefaultAlpha
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.drawscope.DrawScope.Companion.DefaultFilterQuality
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.ContentScale
import coil.compose.AsyncImage
import coil.compose.AsyncImagePainter.State

@Composable
fun NovaAsyncImage(
    modifier: Modifier = Modifier,
    model: Any?,
    contentDescription: String? = "",
    placeholder: Painter? = null,
    error: Painter? = null,
    fallback: Painter? = error,
    onLoading: ((State.Loading) -> Unit)? = null,
    onSuccess: ((State.Success) -> Unit)? = null,
    onError: ((State.Error) -> Unit)? = null,
    alignment: Alignment = Alignment.Center,
    contentScale: ContentScale = ContentScale.Fit,
    alpha: Float = DefaultAlpha,
    colorFilter: ColorFilter? = null,
    filterQuality: FilterQuality = DefaultFilterQuality,
) {
    AsyncImage(
        model = model,
        contentDescription = contentDescription,
        modifier = modifier,
        placeholder = placeholder,
        error = error,
        fallback = fallback,
        onLoading = onLoading,
        onSuccess = onSuccess,
        onError = onError,
        alignment = alignment,
        contentScale = contentScale,
        alpha = alpha,
        colorFilter = colorFilter,
        filterQuality = filterQuality,
    )
}
