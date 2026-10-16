package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.sqrt

/**
 * Every coin the strip draws, as a persistent object with a spring on each channel.
 *
 * An arrangement change only moves targets, so a tap mid-flight re-aims rather than restarting, and a coin
 * keeps its identity across relayouts. Ported from `src/coin/coin-field.js`.
 */
class CoinageCoinField {
    /** One coin's channels. Every one of these is chased, not set. */
    class Channels {
        val centreX = CoinageSpring()
        val centreY = CoinageSpring()
        val lift = CoinageSpring()
        val height = CoinageSpring()
        val turn = CoinageSpring()
        val tilt = CoinageSpring()
        val thickness = CoinageSpring(value = 1f)
        val wear = CoinageSpring()
        val luster = CoinageSpring()
        val calm = CoinageSpring()
    }

    /** Where one coin is heading. */
    data class Target(
        val centreX: Float,
        val centreY: Float,
        val height: Float,
        val turn: Float,
        val thickness: Float,
        val wear: Float,
        val luster: Float,
        val calm: Float,
        val tilt: Float = 0f,
        /** Toward the viewer. */
        val lift: Float = 0f
    )

    class Member(
        /**
         * Re-read on every retarget rather than kept: an id names a holding, and a holding's payment history
         * can grow under it. Leaving the first reading in place would draw a coin's pits from whatever it
         * was worth the first time the field saw it.
         */
        var coin: CoinageScene.Coin,
        /**
         * Scatters this coin's pits and streaks. Taken from its identity, so a coin keeps the same face
         * across frames and across relayouts without anything being stored.
         */
        val seed: Float,
        val channels: Channels,
        var target: Target,
        /** Staggers this coin's start across an arrangement change, in display order. */
        var hold: Float
    )

    var members: List<Member> = emptyList()
        private set

    private var index: Map<String, Int> = emptyMap()

    /** True while any coin is still travelling, which is what decides whether to draw a frame. */
    var isMoving = false
        private set

    /**
     * Which end of the field sets off first.
     *
     * Coins leave in display order and return in reverse, so in both directions the ones with furthest to go
     * start first. Spreading into the grid the far end has the long haul, and gathering back into the strip
     * it is the near end; taking the same end each way left the long travellers setting off last.
     */
    enum class Stagger {
        FROM_FRONT,
        FROM_BACK;

        fun position(order: Int, count: Int): Int = when (this) {
            FROM_FRONT -> order
            FROM_BACK -> count - 1 - order
        }
    }

    /**
     * Re-aims every coin. A coin already in the field keeps where it has got to and simply gets a new
     * destination. A coin new to the field arrives from off the right edge at a fraction of its size and
     * fully worn, and settles in.
     */
    fun retarget(targets: List<Pair<CoinageScene.Coin, Target>>, spawnEdge: Float, stagger: Stagger) {
        val updated = ArrayList<Member>(targets.size)
        val lookup = HashMap<String, Int>(targets.size)
        val count = maxOf(targets.size, 1)

        targets.forEachIndexed { order, (coin, target) ->
            val hold = stagger.position(order, count).toFloat() / count * CoinageSpring.STAGGER
            val existing = index[coin.id]?.takeIf { it < members.size }?.let { members[it] }

            if (existing != null) {
                existing.coin = coin
                existing.target = target
                existing.hold = hold
                updated += existing
            } else {
                updated += Member(
                    coin = coin,
                    seed = seed(coin.id),
                    channels = seeded(target, spawnEdge),
                    target = target,
                    hold = hold
                )
            }

            lookup[coin.id] = updated.size - 1
        }

        members = updated
        index = lookup
        isMoving = true
    }

    /** Advances every spring. Returns whether anything is still moving, so a settled field can stop asking. */
    fun advance(seconds: Float): Boolean {
        var moving = false

        for (member in members) {
            val step = maxOf(seconds - member.hold, 0f)
            member.hold = maxOf(member.hold - seconds, 0f)

            if (step <= 0f) {
                moving = true
                continue
            }

            moving = advance(member, step) || moving
        }

        isMoving = moving

        return moving
    }

    /** How far a coin still has to travel, which decides how much luster it has picked up. */
    fun distanceToTarget(member: Member): Float {
        val across = member.channels.centreX.value - member.target.centreX
        val down = member.channels.centreY.value - member.target.centreY

        return sqrt(across * across + down * down)
    }

    /**
     * Where a coin new to the field starts: off the right edge, small, and as traceable as a coin can be, so
     * arriving money visibly settles into the strip and then clears.
     */
    private fun seeded(target: Target, edge: Float) = Channels().apply {
        centreX.value = edge + SPAWN_OFFSET
        centreY.value = target.centreY
        height.value = target.height * SPAWN_SCALE
        turn.value = target.turn
        tilt.value = target.tilt
        thickness.value = target.thickness
        wear.value = CoinageWear.unknown
        calm.value = target.calm
    }

    private fun advance(member: Member, seconds: Float): Boolean {
        val target = member.target
        val channels = member.channels

        channels.centreX.step(target.centreX, seconds, CoinageSpring.Omega.POSITION)
        channels.centreY.step(target.centreY, seconds, CoinageSpring.Omega.POSITION)
        channels.height.step(target.height, seconds, CoinageSpring.Omega.POSITION)
        channels.lift.step(target.lift, seconds, CoinageSpring.Omega.POSITION)
        channels.turn.step(target.turn, seconds, CoinageSpring.Omega.ROTATION)
        channels.tilt.step(target.tilt, seconds, CoinageSpring.Omega.ROTATION)
        channels.thickness.step(target.thickness, seconds, CoinageSpring.Omega.THICKNESS)
        channels.calm.step(target.calm, seconds, CoinageSpring.Omega.THICKNESS)
        channels.luster.step(target.luster, seconds, CoinageSpring.Omega.LUSTER)
        channels.wear.step(target.wear, seconds, CoinageSpring.Omega.WEAR)

        return !settled(member)
    }

    private fun settled(member: Member): Boolean {
        val target = member.target
        val channels = member.channels

        return channels.centreX.isSettled(target.centreX) &&
            channels.centreY.isSettled(target.centreY) &&
            channels.height.isSettled(target.height) &&
            channels.turn.isSettled(target.turn) &&
            channels.tilt.isSettled(target.tilt) &&
            channels.thickness.isSettled(target.thickness) &&
            channels.calm.isSettled(target.calm) &&
            channels.lift.isSettled(target.lift) &&
            channels.luster.isSettled(target.luster) &&
            channels.wear.isSettled(target.wear)
    }

    companion object {
        private const val SPAWN_OFFSET = 30f
        private const val SPAWN_SCALE = 0.4f

        fun seed(id: String): Float {
            var hashed = 0x9E3779B97F4A7C15uL

            for (character in id) {
                hashed = hashed * 31uL + character.code.toULong()
            }

            return (hashed % 100_000uL).toFloat() / 1_000f
        }
    }
}
