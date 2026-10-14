package io.paritytech.polkadotapp.feature_videogame_impl.data

import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds

object VideoGameTimings {
    val HOST_INTRODUCTION = 2.seconds
    val HOST_ACTIVE_MINIMUM = 8.seconds
    val HOST_ENDING = 2.seconds

    /**
     * Window before a phase boundary during which the next phase's peers are pre-warmed.
     *
     * Kept short so it doesn't span an entire round when group sizes are small
     * (a 2-player round is HOST_FULL_CYCLE * 2 = 24 s, less than 30 s would have been).
     *
     * For round-0 pre-warm during WaitingRoom this MUST stay <= the runtime's `post_shuffle_margin`
     * (currently 30 s on next-people-paseo / people-westend); otherwise the on-chain state may
     * still be `Shuffle` when this window opens and `gameInfo.state` will not yet be `InProgress`,
     * silently degrading round 0 to a cold connect.
     */
    val PRE_CONNECTION_TIME = 10.seconds

    val CONNECTION_STAGE_DURATION = 20.seconds

    val HOST_FULL_CYCLE = HOST_INTRODUCTION + HOST_ACTIVE_MINIMUM + HOST_ENDING

    val WAITING_ROOM_AVAILABLE_BEFORE = 5.minutes

    val NO_HOST_TOOLTIP_DURATION = 4.seconds
}
