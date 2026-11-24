package io.paritytech.polkadotapp.feature_products_api.model

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.LayoutCoordinates
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Whether a mask's shapes hide the node or are all of it that shows. */
@Serializable
enum class JsMaskMode {
    @SerialName("hide")
    HIDE,

    @SerialName("show")
    SHOW,
}

/** A place in a masked node, in hundredths of a dp from its top-start corner, as the wire carries it. */
@Serializable
data class JsMaskPoint(val x: Int, val y: Int)

/** One shape of a mask; places and sizes in hundredths of a dp. */
@Serializable
sealed interface JsMaskShape {
    /** The whole node. */
    @Serializable
    @SerialName("all")
    data object All : JsMaskShape

    @Serializable
    @SerialName("circle")
    data class Circle(val center: JsMaskPoint, val radius: Long) : JsMaskShape

    /** A line through [points], [width] wide, with round caps and joins; one point is a dot. */
    @Serializable
    @SerialName("stroke")
    data class Stroke(val width: Long, val points: List<JsMaskPoint>) : JsMaskShape

    @Serializable
    @SerialName("stamped")
    data class Stamped(val width: Long, val brush: JsMaskBrush, val points: List<JsMaskPoint>) : JsMaskShape
}

/** The location pan whose strokes the host draws into the mask itself, and their width in hundredths of a dp. */
@Serializable
data class JsMaskFollow(val pan: String, val width: Long, val brush: JsMaskBrush? = null)

@Serializable
enum class JsMaskBrushTip {
    @SerialName("ellipse")
    ELLIPSE,

    @SerialName("capsule")
    CAPSULE,
}

@Serializable
data class JsMaskBrush(
    val tip: JsMaskBrushTip,
    val aspectPermille: Int,
    val angleCentideg: Int,
    val spacingPermille: Int,
)

/**
 * Has the host send [action], with no payload, once a mask's shapes cover [atPermille] thousandths of its node: the
 * share cleared for a hide mask, the share shown for a show mask.
 */
@Serializable
data class JsClearedShare(
    val atPermille: Int,
    val action: String,
    val whileDown: Boolean = false,
    val rescue: JsShareRescue? = null,
) {
    /** Whether [covered] samples of [samples] reach the share, as the core counts it. */
    fun reached(covered: Int, samples: Int): Boolean = covered.toLong() * PERMILLE >= atPermille.toLong() * samples

    private companion object {
        const val PERMILLE = 1000L
    }
}

@Serializable
data class JsShareRescue(val afterMs: Long, val fromPermille: Int)

@Serializable
enum class JsMaskHaptic { TICK, LIGHT, MEDIUM, HEAVY, SUCCESS }

@Serializable
data class JsMaskHaptics(val everyPermille: Int, val step: JsMaskHaptic, val reach: JsMaskHaptic?)

/**
 * What one keyed mask gained since its tree (`RendererNode.ExtendMask`), and its fade into the next tree, excluded
 * from serialized widget trees. Values are written where an answer arrives (the render stream's thread) as snapshot
 * state, so a frame that waited for a move's answer draws it with no hop through a collector.
 */
class JsMaskShapes {
    data class Fade(val from: Picture, val startNanos: Long, val durationNanos: Long)

    /**
     * Everything a mask draws at one moment: its mode and shapes, the followed strokes among them, all in hundredths
     * of a dp from the node's top-start corner.
     */
    data class Picture(val mode: JsMaskMode, val shapes: List<JsMaskShape>)

    private val addedState = mutableStateOf(emptyList<JsMaskShape>())
    private val fadeState = mutableStateOf<Fade?>(null)
    private val additionsState = mutableIntStateOf(0)
    private val treesState = mutableIntStateOf(0)
    var treeArrivedAtNanos by mutableLongStateOf(System.nanoTime())
        private set

    /** How many times shapes were added, and how many trees arrived, since this was made: when to check a share. */
    val additions: Int get() = additionsState.intValue
    val trees: Int get() = treesState.intValue

    /** The shapes added since the current tree, read in the draw phase. */
    val added: List<JsMaskShape> get() = addedState.value

    val fade: Fade? get() = fadeState.value

    /** What the mask's view draws from besides this: kept by the view, to know what was drawn when a tree arrives. */
    @Volatile
    var source: (() -> Picture)? = null

    fun extend(shapes: List<JsMaskShape>) {
        if (shapes.isEmpty()) return
        synchronized(this) {
            addedState.value = addedState.value + shapes
            additionsState.intValue += 1
        }
    }

    /**
     * A new tree arrived whose mask with this key is [next] (null when it has none): it sets the shapes itself. A
     * change fades from what was drawn, strokes included, when [next] asks for a fade; no change leaves a running fade
     * alone. Called before the trails are dropped.
     */
    fun treeArrived(next: JsModifier.Mask?, nowNanos: Long = System.nanoTime()) {
        val before = source?.invoke()
        synchronized(this) {
            if (addedState.value.isNotEmpty()) addedState.value = emptyList()
            treesState.intValue += 1
            treeArrivedAtNanos = nowNanos
        }
        if (next == null) {
            fadeState.value = null
            return
        }
        val after = Picture(next.mode, next.shapes)
        if (before == null || before == after) return
        fadeState.value = if (next.fadeMs > 0) Fade(before, nowNanos, next.fadeMs * NANOS_PER_MS) else null
    }

    /** Ends the fade once it is over, so the mask stops redrawing every frame. */
    fun endFade(nowNanos: Long = System.nanoTime()) {
        val fade = fadeState.value ?: return
        if (nowNanos - fade.startNanos >= fade.durationNanos) fadeState.value = null
    }

    private companion object {
        const val NANOS_PER_MS = 1_000_000L
    }
}

/**
 * The finger's strokes on one keyed location pan since the current tree, in px from the pan's top-left corner
 * (whatever the layout direction), and where that corner is in the root, for masks that follow the pan. Written at
 * the touch, so a following mask draws a move in that touch's frame. A tree drops them.
 */
class JsPanTrail {
    internal var coordinates: LayoutCoordinates? = null

    /** Invalidates followed masks as a pressed gesture changes its children's coordinate transform. */
    internal var drawnScale by mutableStateOf(Offset(1f, 1f))

    private val strokesState = mutableStateOf(emptyList<List<Offset>>())

    val strokes: List<List<Offset>> get() = strokesState.value

    /** The pan's top-left corner in the root, px. */
    var origin by mutableStateOf(Offset.Zero)

    /** Whether a finger is on the pan, and how many times one has lifted from it: when a following mask checks its share. */
    var down by mutableStateOf(false)
        private set
    var lifts by mutableIntStateOf(0)
        private set

    /** The finger touched the pan ([isDown]) or left it. A tree leaves this as it is: the finger has not moved. */
    fun touch(isDown: Boolean) {
        if (isDown == down) return
        down = isDown
        if (!isDown) lifts += 1
    }

    fun begin(at: Offset) {
        strokesState.value = strokesState.value + listOf(listOf(at))
    }

    /** The finger reached [point] on the current stroke; with no stroke begun, it starts one. */
    fun extend(point: Offset) {
        val strokes = strokesState.value
        val last = strokes.lastOrNull() ?: return begin(point)
        if (last.last() == point) return
        strokesState.value = strokes.dropLast(1) + listOf(last + point)
    }

    fun reset() {
        if (strokesState.value.isNotEmpty()) strokesState.value = emptyList()
    }
}
