package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.database.ContentObserver
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import io.paritytech.polkadotapp.feature_products_api.model.JsEnter
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsTrack

/** An enclosing host can ask for less motion too; otherwise the renderer follows Android's animator setting. */
internal val LocalRendererMotionEnabled = staticCompositionLocalOf<Boolean?> { null }

@Composable
internal fun rendererMotionEnabled(): Boolean {
    val inherited = LocalRendererMotionEnabled.current
    val resolver = LocalContext.current.contentResolver
    fun enabled() = Settings.Global.getFloat(resolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f
    var platformEnabled by remember(resolver) { mutableStateOf(enabled()) }
    DisposableEffect(resolver, inherited) {
        val observer = object : ContentObserver(Handler(Looper.getMainLooper())) {
            override fun onChange(selfChange: Boolean) {
                platformEnabled = enabled()
            }
        }
        platformEnabled = enabled()
        if (inherited != false) resolver.registerContentObserver(Settings.Global.getUriFor(Settings.Global.ANIMATOR_DURATION_SCALE), false, observer)
        onDispose { if (inherited != false) resolver.unregisterContentObserver(observer) }
    }
    return platformEnabled && inherited != false
}

/** The Enter and tracks a node asks for, read once from its decoded modifiers, which are never changed afterwards. */
@Immutable
internal data class NodeMotionSpec(val enter: JsEnter?, val tracks: List<JsTrack>)

internal fun List<JsModifier>.nodeMotionSpec() = NodeMotionSpec(
    enter = filterIsInstance<JsModifier.Enter>().lastOrNull()?.enter,
    tracks = filterIsInstance<JsModifier.Animate>().map { it.track },
)

@Composable
internal fun nodeMotionModifier(spec: NodeMotionSpec): Modifier {
    val tracks = spec.tracks
    val player = remember { NodeMotionPlayer(spec.enter, tracks) }
    val enabled = LocalRendererMotionEnabled.current ?: rendererMotionEnabled()
    val density = LocalDensity.current.density
    val direction = if (LocalLayoutDirection.current == LayoutDirection.Rtl) -1 else 1
    var pose by remember { mutableStateOf(player.initialPose(enabled)) }
    SideEffect {
        player.update(tracks)
        pose = player.preview(enabled)
    }
    LaunchedEffect(tracks, enabled) {
        if (!enabled) {
            pose = player.initialPose(animate = false)
            withFrameNanos { player.sample(it, animate = false) }
        } else {
            do {
                val frame = withFrameNanos { now ->
                    player.sample(now, animate = true)
                }
                pose = frame.pose
            } while (frame.running)
        }
    }
    return if (player.hasEnter || tracks.isNotEmpty()) {
        Modifier.graphicsLayer {
            val current = pose
            alpha = current.opacity
            scaleX = current.scale
            scaleY = current.scale
            translationX = current.x * density * direction
            translationY = current.y * density
            rotationZ = current.rotation
        }
    } else {
        Modifier
    }
}
