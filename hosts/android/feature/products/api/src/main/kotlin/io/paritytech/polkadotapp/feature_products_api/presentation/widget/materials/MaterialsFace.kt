package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.graphics.Bitmap
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.semantics.clearAndSetSemantics
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsUiEventHandler
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageLoaded
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsMaterialsRenderer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.withContext
import kotlin.time.Duration.Companion.milliseconds

/**
 * Draws [face] with the shader layers [resolution] settles for its `Materials` nodes, held across faces by [materials].
 *
 * The layers read a picture of the face drawn plain over [plainBackground], which this keeps current as the face and its
 * images change. With no resolution, or while every node of it draws plain, the face draws as a plain [JsWidgetRenderer].
 * [modifier] sizes the face, and the picture takes the same size.
 */
@Composable
fun MaterialsFace(
    face: JsWidget,
    resolution: MaterialsResolution?,
    materials: CardMaterialsSlot,
    jsEventHandler: JsUiEventHandler,
    modifier: Modifier = Modifier,
    plainBackground: @Composable BoxScope.() -> Unit = {},
) {
    if (resolution != null && materials.drawsMaterialsOf(resolution)) {
        LayeredFace(face, remember(resolution) { materials.of(resolution) }, jsEventHandler, modifier, plainBackground)
    } else {
        JsWidgetRenderer(widget = face, modifier = modifier, jsEventHandler = jsEventHandler)
    }
}

@Composable
private fun CardMaterialsSlot.drawsMaterialsOf(resolution: MaterialsResolution): Boolean {
    val resolved by resolution.resolved.collectAsState()
    return remember(resolved) { drawsMaterials(resolved) }
}

@Composable
private fun LayeredFace(
    face: JsWidget,
    materials: CardMaterials,
    jsEventHandler: JsUiEventHandler,
    modifier: Modifier,
    plainBackground: @Composable BoxScope.() -> Unit,
) {
    val plainFace = rememberGraphicsLayer()
    val captureRequests = remember { Channel<Unit>(Channel.CONFLATED) }
    val imageLoaded = remember<() -> Unit> { { captureRequests.trySend(Unit) } }
    val facePicture = remember { mutableStateOf<ImageBitmap?>(null) }
    val faceCoordinates = remember { mutableStateOf<LayoutCoordinates?>(null) }

    // The copy asks for a picture when it redraws, which a new face does. An image loads into a layer of its
    // own that redraws without the copy, so it asks once it has loaded.
    LaunchedEffect(plainFace) {
        captureRequests.receiveAsFlow().collect {
            withFrameNanos { }
            if (plainFace.size.width == 0 || plainFace.size.height == 0) return@collect
            val captured = runCancellableCatching { plainFace.toImageBitmap() }
                .logFailure("materials: the plain face was not captured")
                .getOrNull() ?: return@collect
            val shown = facePicture.value?.asAndroidBitmap()
            val changed = withContext(Dispatchers.Default) {
                captured.asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false)?.takeUnless { shown?.sameAs(it) == true }
            }
            if (changed != null) facePicture.value = changed.asImageBitmap()
            delay(BASE_CAPTURE_INTERVAL)
        }
    }

    Box {
        Box(
            modifier = Modifier
                .matchParentSize()
                .clearAndSetSemantics {}
                .focusProperties { canFocus = false }
                .onGloballyPositioned { faceCoordinates.value = it }
                .drawWithContent {
                    plainFace.record { this@drawWithContent.drawContent() }
                    captureRequests.trySend(Unit)
                }
        ) {
            CompositionLocalProvider(LocalJsImageLoaded provides imageLoaded) {
                plainBackground()
                JsWidgetRenderer(widget = face, modifier = Modifier.matchParentSize(), jsEventHandler = { _, _ -> })
            }
        }

        val renderer = remember(materials) { CardMaterialsRenderer(materials, facePicture, faceCoordinates) }
        CompositionLocalProvider(LocalJsMaterialsRenderer provides renderer) {
            JsWidgetRenderer(widget = face, modifier = modifier, jsEventHandler = jsEventHandler)
        }
    }
}

private val BASE_CAPTURE_INTERVAL = 250.milliseconds
