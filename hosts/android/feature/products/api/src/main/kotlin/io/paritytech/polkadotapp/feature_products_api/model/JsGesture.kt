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
)

/** The stage a gesture action reports. */
enum class JsGesturePhase { TAP, STARTED, MOVED, ENDED, CANCELLED, COMPLETED }
