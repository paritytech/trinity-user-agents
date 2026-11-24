package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.graphics.drawscope.translate
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import coil.compose.AsyncImagePainter
import coil.compose.rememberAsyncImagePainter
import coil.request.ImageRequest
import io.paritytech.polkadotapp.feature_products_api.model.JsBurst
import io.paritytech.polkadotapp.feature_products_api.model.JsBurstTrigger
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitSource
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitter
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsParticleLook
import kotlinx.coroutines.flow.first
import kotlin.math.atan2
import kotlin.math.hypot
import kotlin.math.roundToInt

/** The actions the renderer sends, heard as they go out so emitters burst with no round trip to the worker. */
internal class ParticleActions {
    private val listeners = mutableListOf<(String) -> Unit>()
    private var muted = 0

    fun listen(listener: (String) -> Unit): () -> Unit {
        listeners += listener
        return { listeners -= listener }
    }

    fun sent(action: String) {
        if (muted == 0) listeners.toList().forEach { it(action) }
    }

    /** Runs [send] without telling emitters: a tree fading out is no longer the current tree. */
    fun muted(send: () -> Unit) {
        muted += 1
        try {
            send()
        } finally {
            muted -= 1
        }
    }
}

internal val LocalParticleActions = staticCompositionLocalOf<ParticleActions?> { null }

/** True inside a child fading out of an old tree: its emitters send no more particles. */
internal val LocalParticlesLeaving = staticCompositionLocalOf { false }

internal val LocalParticleOverlay = staticCompositionLocalOf<ParticleOverlay?> { null }

/**
 * Draws a node's particles over all it draws, in its frame and unclipped, and sends them from its emitter. Particles
 * outlive their emitter: a tree that drops it lets them finish here, and a node that leaves hands them to the
 * renderer's [ParticleOverlay]. Nothing is sent where animations are off or from a tree fading out.
 */
@Composable
internal fun particleModifier(emit: JsModifier.Emit?): Modifier {
    val node = remember { NodeParticles() }
    if (emit == null && !node.started) return Modifier
    node.started = true
    val enabled = LocalRendererMotionEnabled.current ?: rendererMotionEnabled()
    val leaving = LocalParticlesLeaving.current
    val emitter = emit?.emitter?.takeIf { enabled && !leaving }
    node.density = LocalDensity.current.density
    node.rtl = LocalLayoutDirection.current == LayoutDirection.Rtl
    val overlay = LocalParticleOverlay.current
    val actions = LocalParticleActions.current
    val coordinates = remember(emit?.trail) { MaskCoordinates() }

    emitter?.looks?.filterIsInstance<JsParticleLook.Image>()?.forEach { look -> key(look) { ParticleImage(look, node) } }

    SideEffect {
        node.trail = emit?.trail
        node.coordinates = coordinates
        val bursts = emit?.emitter?.bursts
        if (emitter != null) {
            val appear = if (node.appeared) emptyList() else emitter.bursts.filter { it.on == JsBurstTrigger.Appear }
            node.queue(appear + ParticleRules.changed(node.previousBursts, emitter.bursts))
        }
        node.appeared = true
        node.previousBursts = bursts
        node.emitter = emitter
    }

    DisposableEffect(actions, node) {
        val stop = actions?.listen { action ->
            node.emitter?.let { live -> node.queue(live.bursts.filter { (it.on as? JsBurstTrigger.Action)?.action == action }) }
        }
        onDispose { stop?.invoke() }
    }

    DisposableEffect(overlay, node) {
        onDispose { overlay?.adopt(node) }
    }

    LaunchedEffect(node) {
        while (true) {
            snapshotFlow { node.busy() }.first { it }
            withFrameNanos(node::step)
        }
    }

    return Modifier
        .onGloballyPositioned { node.layout(it, overlay?.coordinates) }
        .drawWithContent {
            drawContent()
            drawParticles(node.system.particles, node.frame, node.density, node.rtl, node.widthPx, node.painters)
        }
}

/** Loads an image look at its own size, keeping the picture for drawing once it is there. */
@Composable
private fun ParticleImage(look: JsParticleLook.Image, node: NodeParticles) {
    val source = look.source ?: return
    val resolver = LocalJsImageResolver.current
    val model by produceState(resolver.resolved(source), source, resolver) { value = resolver.resolve(source) ?: value }
    val located = model ?: return
    val density = LocalDensity.current.density
    val painter = rememberAsyncImagePainter(
        ImageRequest.Builder(LocalContext.current)
            .data(located)
            .size(look.width.px(density).coerceAtLeast(1), look.height.px(density).coerceAtLeast(1))
            .build(),
    )
    val state = painter.state
    SideEffect { if (state is AsyncImagePainter.State.Success) node.painters[look] = state.painter }
}

private fun Long.px(density: Float): Int = (this * density / HUNDREDTHS).roundToInt()

/** One node's particles and what drawing and sending them needs between frames. */
internal class NodeParticles {
    val system = ParticleSystem()
    val painters = HashMap<JsParticleLook, Painter>()
    var started = false
    var appeared = false
    var previousBursts: List<JsBurst>? = null
    var trail: JsPanTrail? = null
    var coordinates: MaskCoordinates? = null
    var density = 1f
    var rtl = false
    var widthPx = 0f
    var heightPx = 0f
    private var topLeft = Offset.Zero
    private var laidOut = false
    private val pending = ArrayList<Int>()

    /** The emitter that may send now: null with none, with animations off, or in a tree fading out. */
    var emitter by mutableStateOf<JsEmitter?>(null)
    private var wake by mutableIntStateOf(0)
    var frame by mutableLongStateOf(0L)
        private set

    /** Where the node's frame lands in the overlay, for particles it hands over when it leaves. */
    var inOverlay: MaskProjection? = null
        private set

    fun queue(bursts: List<JsBurst>) {
        if (bursts.isEmpty()) return
        bursts.forEach { pending += it.count }
        wake += 1
    }

    fun busy(): Boolean {
        @Suppress("UNUSED_VARIABLE", "unused")
        val woken = wake
        val live = emitter ?: return !system.isEmpty
        val sending = when (live.source) {
            is JsEmitSource.Pan -> trail?.down == true
            else -> live.ratePerS > 0
        }
        return sending || pending.isNotEmpty() || !system.isEmpty
    }

    fun layout(node: LayoutCoordinates, overlay: LayoutCoordinates?) {
        widthPx = node.size.width.toFloat()
        heightPx = node.size.height.toFloat()
        topLeft = node.positionInRoot()
        laidOut = true
        coordinates?.node = node
        if (overlay != null && overlay.isAttached && node.isAttached) {
            val origin = overlay.localPositionOf(node, Offset.Zero)
            inOverlay = MaskProjection(origin, overlay.localPositionOf(node, Offset(1f, 0f)) - origin, overlay.localPositionOf(node, Offset(0f, 1f)) - origin)
        }
    }

    fun step(now: Long) {
        if (!laidOut) return
        val live = emitter
        val width = widthPx / density * HUNDREDTHS
        val height = heightPx / density * HUNDREDTHS
        system.step(now, live, width.toDouble(), height.toDouble(), if (live?.source is JsEmitSource.Pan) finger() else null)
        if (live != null) pending.forEach { system.burst(live, it, now, width.toDouble(), height.toDouble()) }
        pending.clear()
        frame = now
    }

    /** Where the finger is on the followed pan, in the node's frame, or null while it is up. */
    private fun finger(): ParticlePoint? {
        val trail = trail ?: return null
        if (!trail.down) return null
        // A new tree drops the strokes; until the finger moves again it is where it last was.
        val point = trail.strokes.lastOrNull()?.lastOrNull() ?: return system.lastFinger
        val coordinates = coordinates
        coordinates?.update(trail.coordinates)
        val projection = coordinates?.projection
        val inNode = projection?.point(point) ?: (point + trail.origin - topLeft)
        val x = inNode.x / density * HUNDREDTHS
        return ParticlePoint((if (rtl) widthPx / density * HUNDREDTHS - x else x).toDouble(), (inNode.y / density * HUNDREDTHS).toDouble())
    }
}

/** Particles whose node has left, drawn over the whole renderer until their lives end. */
internal class ParticleOverlay {
    var coordinates: LayoutCoordinates? = null
        private set
    private val groups = mutableStateListOf<Orphans>()
    private var frame by mutableLongStateOf(0L)

    private class Orphans(
        val particles: MutableList<Particle>,
        val projection: MaskProjection,
        val rtl: Boolean,
        val widthPx: Float,
        val density: Float,
        val painters: Map<JsParticleLook, Painter>,
    )

    fun adopt(node: NodeParticles) {
        val projection = node.inOverlay ?: return
        if (node.system.isEmpty) return
        frame = maxOf(frame, node.frame)
        groups += Orphans(ArrayList(node.system.particles), projection, node.rtl, node.widthPx, node.density, HashMap(node.painters))
        node.system.particles.clear()
    }

    val modifier: Modifier = Modifier
        .onGloballyPositioned { coordinates = it }
        .drawWithContent {
            drawContent()
            val now = frame
            for (group in groups) {
                val horizontal = group.projection.horizontal
                drawParticles(
                    group.particles, now, group.density, group.rtl, group.widthPx, group.painters,
                    place = group.projection::point,
                    scale = hypot(horizontal.x, horizontal.y),
                    turn = Math.toDegrees(atan2(horizontal.y, horizontal.x).toDouble()).toFloat(),
                )
            }
        }

    suspend fun run() {
        while (true) {
            snapshotFlow { groups.isNotEmpty() }.first { it }
            withFrameNanos { now ->
                groups.removeAll { group ->
                    group.particles.removeAll { ParticleRules.place(it, now) == null }
                    group.particles.isEmpty()
                }
                frame = now
            }
        }
    }
}

/**
 * Draws [particles] as they are at [now], from the node's frame [widthPx] wide (mirrored right to left) through
 * [place] into the canvas, grown by [scale] and turned by [turn] degrees as that frame is.
 */
@Suppress("LongParameterList")
private fun DrawScope.drawParticles(
    particles: List<Particle>,
    now: Long,
    density: Float,
    rtl: Boolean,
    widthPx: Float,
    painters: Map<JsParticleLook, Painter>,
    place: (Offset) -> Offset = { it },
    scale: Float = 1f,
    turn: Float = 0f,
) {
    for (particle in particles) {
        val at = ParticleRules.place(particle, now) ?: continue
        val x = (at.x * density / HUNDREDTHS).toFloat()
        val centre = place(Offset(if (rtl) widthPx - x else x, (at.y * density / HUNDREDTHS).toFloat()))
        val grow = (at.scale * scale).toFloat()
        val alpha = at.opacity.toFloat().coerceIn(0f, 1f)
        if (grow <= 0f || alpha <= 0f) continue
        val rotation = (if (rtl) -at.rotation else at.rotation).toFloat() + turn
        fun size(width: Long, height: Long) = Size(width * density / HUNDREDTHS * grow, height * density / HUNDREDTHS * grow)
        when (val look = particle.look) {
            is JsParticleLook.Circle -> drawCircle(WidgetStyleRules.color(look.rgba), size(look.diameter, look.diameter).width / 2, centre, alpha)
            is JsParticleLook.Rectangle -> rotate(rotation, centre) {
                val box = size(look.width, look.height)
                drawRect(WidgetStyleRules.color(look.rgba), centre - Offset(box.width / 2, box.height / 2), box, alpha)
            }
            is JsParticleLook.Image -> {
                val painter = painters[look] ?: continue
                val box = size(look.width, look.height)
                rotate(rotation, centre) {
                    translate(centre.x - box.width / 2, centre.y - box.height / 2) {
                        with(painter) { draw(box, alpha) }
                    }
                }
            }
        }
    }
}

private const val HUNDREDTHS = 100f
