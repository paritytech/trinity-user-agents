package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** The direction a pan accepts. */
@Serializable
enum class JsPanAxis {
    @SerialName("horizontal")
    HORIZONTAL,

    @SerialName("vertical")
    VERTICAL,

    @SerialName("free")
    FREE,
}

/** How far the host may move a dragged child. */
@Serializable
sealed interface JsPanTravel {
    /** The parent's width less the child's own. */
    @Serializable
    @SerialName("parentWidth")
    data object ParentWidth : JsPanTravel

    @Serializable
    @SerialName("points")
    data class Points(val dp: Int) : JsPanTravel
}

/** Where a host-moved child goes when the finger lifts. */
@Serializable
sealed interface JsPanRelease {
    @Serializable
    @SerialName("snapBack")
    data object SnapBack : JsPanRelease

    @Serializable
    @SerialName("stay")
    data object Stay : JsPanRelease

    /** To the end, as a completion, once the drag reached [thresholdPercent] of the travel; back otherwise. */
    @Serializable
    @SerialName("complete")
    data class Complete(val thresholdPercent: Int) : JsPanRelease
}

/** The host moves the child itself during a drag, and keeps it there across trees. */
@Serializable
data class JsPanDrag(
    val travel: JsPanTravel = JsPanTravel.ParentWidth,
    val release: JsPanRelease = JsPanRelease.SnapBack,
    val holdAtEnd: Boolean = true,
)

/**
 * A pan over the node. With [drag] the host moves the child; without it the product hears the moves through
 * [moveAction] and moves the child with new trees.
 */
@Serializable
data class JsPanGesture(
    val axis: JsPanAxis = JsPanAxis.HORIZONTAL,
    val drag: JsPanDrag? = null,
    val startAction: String? = null,
    val moveAction: String? = null,
    val moveIntervalMs: Int = 0,
    val endAction: String? = null,
    /** Events carry where the finger is inside the node, clamped to it, instead of how far it moved. */
    val location: Boolean = false,
)

/**
 * How the children feel under a finger and when let go, by value: grown to [pressScale] over [pressMs] when pressed,
 * back over [unpressMs]; home on a spring of [springOmega] rad/s and [springDamping]; to the end over [approachMs]
 * (longer as the square root of the distance past [approachReference] dp), then the [pop] scales a sixtieth apart,
 * standing up to [popOffset] dp past the end at the peak.
 */
@Serializable
data class JsPanFeel(
    val pressScale: Float = 1f,
    val pressScaleY: Float? = null,
    val pressMs: Int = 0,
    val unpressMs: Int = 0,
    val springOmega: Float = 0f,
    val springDamping: Float = 1f,
    val approachMs: Int = 0,
    val approachReference: Float = 1f,
    val pop: List<Float> = emptyList(),
    val popOffset: Float = 0f,
)

/** The stage a gesture action reports. */
enum class JsGesturePhase { TAP, STARTED, MOVED, ENDED, CANCELLED, COMPLETED }
