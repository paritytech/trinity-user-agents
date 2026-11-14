package io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia

import android.content.Context
import android.graphics.Outline
import android.graphics.Rect
import android.graphics.SurfaceTexture
import android.view.Gravity
import android.view.TextureView
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.webkit.WebView
import android.widget.FrameLayout
import org.webrtc.EglBase
import org.webrtc.EglRenderer
import org.webrtc.GlRectDrawer
import org.webrtc.VideoFrame
import org.webrtc.VideoSink
import org.webrtc.VideoTrack
import uniffi.truapi.*
import java.util.concurrent.CountDownLatch
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/** Pictures are siblings of the WebView. WebView.draw/canvas/JS never traverse these layers. */
internal class NativeMediaCompositor(
    private val egl: EglBase.Context,
    private val viewportChanged: (NativeMediaViewport?) -> Unit,
    private val picture: (String, NativeMediaPictureSource) -> VideoTrack?,
) : AutoCloseable {
    private var webView: WebView? = null
    private var host: ViewGroup? = null
    private var below: FrameLayout? = null
    private var above: FrameLayout? = null
    private var revision = 0uL
    private var viewport: NativeMediaViewport? = null
    private val layouts = mutableMapOf<String, Pair<ULong, List<NativeMediaSurface>>>()
    private val renderers = mutableListOf<PictureView>()
    private val layoutListener = View.OnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> changed() }
    private val attachListener = object : View.OnAttachStateChangeListener {
        override fun onViewAttachedToWindow(view: View) { attachLayers() }
        override fun onViewDetachedFromWindow(view: View) { detachLayers() }
    }

    fun attach(view: WebView) {
        close()
        webView = view
        view.addOnAttachStateChangeListener(attachListener)
        view.addOnLayoutChangeListener(layoutListener)
        if (view.isAttachedToWindow) attachLayers()
    }

    private fun attachLayers() {
        val view = webView ?: return
        val parent = view.parent as? FrameLayout ?: return
        if (host === parent) return
        host = parent
        below = FrameLayout(view.context).apply { isClickable = false; importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS }
        above = FrameLayout(view.context).apply { isClickable = false; importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS }
        parent.addView(below, parent.indexOfChild(view), FrameLayout.LayoutParams(-1, -1))
        parent.addView(above, parent.indexOfChild(view) + 1, FrameLayout.LayoutParams(-1, -1))
        changed()
    }

    private fun changed() {
        val view = webView ?: return
        revision++
        clearPictures()
        layouts.clear()
        @Suppress("DEPRECATION")
        val scale = view.scale.takeIf { it > 0f } ?: view.resources.displayMetrics.density
        viewport = if (host != null && view.width > 0 && view.height > 0) NativeMediaViewport(revision,
            (view.width / scale).toUInt(), (view.height / scale).toUInt(), (scale * 1000).roundToInt().toUInt(), 1000u) else null
        viewportChanged(viewport)
    }

    fun current(): NativeMediaViewport? = viewport
    fun invalidateViewport() = changed()

    fun set(session: String, viewportRevision: ULong, layoutRevision: ULong, surfaces: List<NativeMediaSurface>) {
        checkViewport(viewportRevision)
        val previous = layouts[session]
        if (previous != null && layoutRevision < previous.first) throw MediaDomainFailure(NativeMediaDomainError.StaleLayout(previous.first))
        if (previous != null && layoutRevision == previous.first) {
            if (surfaces.size != previous.second.size || surfaces.indices.any { !sameSurface(surfaces[it], previous.second[it]) }) throw MediaDomainFailure(NativeMediaDomainError.InvalidSurface)
            return
        }
        checkSurfaces(surfaces)
        layouts[session] = layoutRevision to surfaces.toList()
        refresh()
    }

    private fun checkViewport(viewportRevision: ULong) {
        val current = viewport ?: throw MediaDomainFailure(NativeMediaDomainError.SurfaceUnavailable)
        if (viewportRevision != current.revision) throw MediaDomainFailure(NativeMediaDomainError.StaleViewport(current.revision))
    }

    private fun checkSurfaces(surfaces: List<NativeMediaSurface>) {
        if (surfaces.size > 12 || surfaces.map { it.surfaceId }.distinct().size != surfaces.size) throw MediaDomainFailure(NativeMediaDomainError.InvalidSurface)
        surfaces.forEach {
            for (rect in listOf(it.rect, it.clip)) {
                if (rect.width > Int.MAX_VALUE.toUInt() || rect.height > Int.MAX_VALUE.toUInt() || rect.x.toLong() + rect.width.toLong() > Int.MAX_VALUE || rect.y.toLong() + rect.height.toLong() > Int.MAX_VALUE) throw MediaDomainFailure(NativeMediaDomainError.InvalidSurface)
            }
        }
    }

    private fun sameSurface(a: NativeMediaSurface, b: NativeMediaSurface): Boolean {
        val sameSource = when (val source = a.source) {
            is NativeMediaPictureSource.Local -> (b.source as? NativeMediaPictureSource.Local)?.picture == source.picture
            is NativeMediaPictureSource.Remote -> (b.source as? NativeMediaPictureSource.Remote)?.let {
                it.picture == source.picture && it.participantId.contentEquals(source.participantId)
            } == true
        }
        return sameSource && a.surfaceId == b.surfaceId && a.rect == b.rect && a.clip == b.clip &&
            a.cornerRadius == b.cornerRadius && a.placement == b.placement && a.depth == b.depth &&
            a.fit == b.fit && a.mirrored == b.mirrored && a.visible == b.visible
    }

    /** One main-thread view-tree transaction replaces the entire ordered picture set. */
    fun refresh() {
        val view = webView ?: return
        val current = viewport ?: return
        val parent = host ?: return
        parent.suppressLayout(true)
        try {
            clearPictures()
            val density = current.deviceScaleNumerator.toFloat() / current.deviceScaleDenominator.toFloat()
            layouts.flatMap { (session, layout) -> layout.second.map { session to it } }
                .sortedWith(compareBy<Pair<String, NativeMediaSurface>> { it.second.depth }.thenBy { it.first }.thenBy { it.second.surfaceId })
                .forEach { (session, surface) ->
                    if (!surface.visible || surface.rect.width == 0u || surface.rect.height == 0u) return@forEach
                    val track = picture(session, surface.source) ?: return@forEach
                    val target = if (surface.placement == NativeMediaPlacement.BELOW_PRODUCT) below else above
                    val left = (surface.rect.x * density).roundToInt()
                    val top = (surface.rect.y * density).roundToInt()
                    val width = (surface.rect.width.toLong() * density).roundToInt()
                    val height = (surface.rect.height.toLong() * density).roundToInt()
                    val clip = Rect(max(0, (surface.clip.x * density).roundToInt()), max(0, (surface.clip.y * density).roundToInt()),
                        min(view.width, ((surface.clip.x.toLong() + surface.clip.width.toLong()) * density).roundToInt()),
                        min(view.height, ((surface.clip.y.toLong() + surface.clip.height.toLong()) * density).roundToInt()))
                    if (!clip.intersect(left, top, left + width, top + height)) return@forEach
                    val container = FrameLayout(view.context)
                    container.clipChildren = true
                    container.clipBounds = Rect(clip.left - left, clip.top - top, clip.right - left, clip.bottom - top)
                    container.outlineProvider = object : ViewOutlineProvider() {
                        override fun getOutline(v: View, outline: Outline) {
                            outline.setRoundRect(0, 0, width, height, min(surface.cornerRadius.toFloat() * density, min(width, height) / 2f))
                        }
                    }
                    container.clipToOutline = true
                    val renderer = PictureView(view.context, egl, track, surface.mirrored, surface.fit == NativeMediaFit.CONTAIN)
                    container.addView(renderer, FrameLayout.LayoutParams(-1, -1, Gravity.CENTER))
                    target?.addView(container, FrameLayout.LayoutParams(width, height).apply { leftMargin = left; topMargin = top })
                    renderers.add(renderer)
                }
        } finally {
            parent.suppressLayout(false)
        }
    }

    fun remove(session: String) { layouts.remove(session); refresh() }
    private fun clearPictures() {
        renderers.forEach { runCatching { it.close() } }
        renderers.clear()
        below?.removeAllViews(); above?.removeAllViews()
    }
    private fun detachLayers() {
        clearPictures()
        layouts.clear()
        host?.removeView(below); host?.removeView(above)
        below = null; above = null; host = null
        revision++
        viewport = null
        viewportChanged(null)
    }
    override fun close() {
        webView?.removeOnAttachStateChangeListener(attachListener)
        webView?.removeOnLayoutChangeListener(layoutListener)
        detachLayers()
        webView = null
    }
}

private class PictureView(context: Context, egl: EglBase.Context, private val track: VideoTrack, mirrored: Boolean, private val contain: Boolean) : TextureView(context), TextureView.SurfaceTextureListener, VideoSink, AutoCloseable {
    private val renderer = EglRenderer("trusted-media-picture")

    @Volatile private var closed = false

    @Volatile private var frameWidth = 0

    @Volatile private var frameHeight = 0
    init {
        isOpaque = false
        renderer.init(egl, EglBase.CONFIG_PLAIN, GlRectDrawer())
        renderer.setMirror(mirrored)
        surfaceTextureListener = this
        track.addSink(this)
    }
    override fun onFrame(frame: VideoFrame) {
        if (closed) return
        if (frame.rotatedWidth != frameWidth || frame.rotatedHeight != frameHeight) {
            frameWidth = frame.rotatedWidth; frameHeight = frame.rotatedHeight
            post { if (!closed) requestLayout() }
        }
        renderer.onFrame(frame)
    }
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val width = MeasureSpec.getSize(widthMeasureSpec)
        val height = MeasureSpec.getSize(heightMeasureSpec)
        if (contain && frameWidth > 0 && frameHeight > 0) {
            val scale = min(width.toFloat() / frameWidth, height.toFloat() / frameHeight)
            setMeasuredDimension((frameWidth * scale).roundToInt(), (frameHeight * scale).roundToInt())
        } else setMeasuredDimension(width, height)
    }
    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) { renderer.createEglSurface(surface); onSurfaceTextureSizeChanged(surface, width, height) }
    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) { renderer.setLayoutAspectRatio(width.toFloat() / height.coerceAtLeast(1)) }
    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {}
    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        if (closed) return true
        val released = CountDownLatch(1)
        renderer.releaseEglSurface { released.countDown() }
        released.await()
        return true
    }
    override fun close() {
        if (closed) return
        closed = true
        runCatching { track.removeSink(this) }
        runCatching { renderer.clearImage(0f, 0f, 0f, 0f) }
        runCatching { renderer.release() }
    }
}
