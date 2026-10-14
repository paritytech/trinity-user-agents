package io.paritytech.polkadotapp.tools_media_connection_impl

import android.graphics.Bitmap
import android.graphics.RenderEffect
import android.graphics.Shader
import android.os.Build
import androidx.annotation.RequiresApi
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.viewinterop.AndroidView
import io.getstream.webrtc.android.ui.VideoTextureViewRenderer
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrackEffect
import kotlinx.collections.immutable.ImmutableSet
import org.webrtc.EglBase
import org.webrtc.RendererCommon
import org.webrtc.VideoTrack as WebRtcVideoTrack

class RealVideoTrack(
    private val webrtcTrack: WebRtcVideoTrack,
    private val eglContext: EglBase.Context
) : VideoTrack {
    private var viewRenderer: VideoTextureViewRenderer? = null

    @Composable
    override fun Render(
        modifier: Modifier,
        isMirrored: Boolean,
        effects: ImmutableSet<VideoTrackEffect>,
        onFirstFrameRendered: (() -> Unit)?,
        onFrameResolutionChanged: ((videoWidth: Int, videoHeight: Int, rotation: Int) -> Unit)?
    ) {
        val rendererEvents = remember(onFirstFrameRendered, onFrameResolutionChanged) {
            object : RendererCommon.RendererEvents {
                override fun onFirstFrameRendered() {
                    onFirstFrameRendered?.invoke()
                }

                override fun onFrameResolutionChanged(videoWidth: Int, videoHeight: Int, rotation: Int) {
                    onFrameResolutionChanged?.invoke(videoWidth, videoHeight, rotation)
                }
            }
        }

        AndroidView(
            modifier = modifier,
            factory = { context ->
                VideoTextureViewRenderer(context).apply {
                    init(eglContext, rendererEvents)
                    setMirror(isMirrored)
                    webrtcTrack.addSink(this)
                    viewRenderer = this
                    applyEffects(effects)
                }
            },
            update = { renderer -> renderer.applyEffects(effects) },
            onRelease = { renderer ->
                webrtcTrack.removeSink(renderer)
                if (viewRenderer === renderer) {
                    viewRenderer = null
                }
            }
        )
    }

    override fun captureFrame(): Bitmap? {
        return viewRenderer?.bitmap
    }

    override fun dispose() {
        viewRenderer?.let { webrtcTrack.removeSink(it) }
        viewRenderer = null
    }
}

private fun VideoTextureViewRenderer.applyEffects(effects: ImmutableSet<VideoTrackEffect>) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return
    setRenderEffect(effects.toRenderEffect())
}

@RequiresApi(Build.VERSION_CODES.S)
private fun ImmutableSet<VideoTrackEffect>.toRenderEffect(): RenderEffect? = this
    .map { it.toRenderEffect() }
    .reduceOrNull { chained, effect -> RenderEffect.createChainEffect(effect, chained) }

@RequiresApi(Build.VERSION_CODES.S)
private fun VideoTrackEffect.toRenderEffect(): RenderEffect = when (this) {
    VideoTrackEffect.Blur -> RenderEffect.createBlurEffect(BLUR_RADIUS_PX, BLUR_RADIUS_PX, Shader.TileMode.CLAMP)
}

private const val BLUR_RADIUS_PX = 25f
