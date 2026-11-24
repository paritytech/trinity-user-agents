package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptics

/** Counts growth on display frames; the existing share watch owns the once-per-lift/tree action. */
internal class MaskProgressWatch(initial: MaskShareWatch.Input) {
    data class Frame(
        val input: MaskShareWatch.Input,
        val covered: Int,
        val total: Int,
        val treeCovered: Int,
        val treeAtNanos: Long,
        val nowNanos: Long,
        val hasFollowedShapes: Boolean,
    )

    data class Result(val action: Boolean, val step: Boolean)

    private val shareWatch = MaskShareWatch(initial)
    private var tree: Int? = null
    private var treeAt: Long? = null
    private var samples = 0
    private var highWater = 0
    private var hapticPermille = 0
    private var lastGrowth = 0L
    private var rescued = false
    var nextRescueAtNanos: Long? = null
        private set

    fun update(frame: Frame, follows: Boolean, share: JsClearedShare?, haptics: JsMaskHaptics?): Result {
        if (frame.total <= 0) return Result(action = false, step = false)
        val newTree = tree != frame.input.trees || treeAt != frame.treeAtNanos
        if (newTree) {
            shareWatch.treeArrived()
            tree = frame.input.trees
            treeAt = frame.treeAtNanos
            lastGrowth = frame.treeAtNanos
            rescued = false
        }
        if (newTree || samples != frame.total) {
            samples = frame.total
            highWater = frame.treeCovered
            hapticPermille = permille(frame.treeCovered)
        }
        val grew = frame.covered > highWater
        if (grew) {
            highWater = frame.covered
            lastGrowth = frame.nowNanos
        }
        val cover = permille(highWater)
        val every = haptics?.everyPermille ?: 0
        val step = grew && every > 0 && cover / every > hapticPermille / every
        hapticPermille = cover
        val rescue = share?.rescue
        val eligible = rescue != null && !rescued && rescue.fromPermille <= share.atPermille &&
            frame.covered.toLong() * 1000 >= rescue.fromPermille.toLong() * frame.total
        nextRescueAtNanos = if (eligible) lastGrowth + requireNotNull(rescue).afterMs * 1_000_000L else null
        val due = nextRescueAtNanos?.let { frame.nowNanos >= it } == true
        if (due) {
            rescued = true
            nextRescueAtNanos = null
        }
        val action = share != null && shareWatch.update(
            frame.input,
            follows,
            whileDown = share.whileDown && frame.hasFollowedShapes,
            force = due,
        ) { share.reached(frame.covered, frame.total) }
        return Result(action, step)
    }

    private fun permille(covered: Int): Int = (covered.toLong() * 1000 / samples).toInt()
}
