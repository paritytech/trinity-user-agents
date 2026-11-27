package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

/** Hooks for timing a gravity reading through to the glass. Times are on the [System.nanoTime] clock. */
interface CoinageFrameTimings {
    /** A reading moved the studio. On the sensor thread. */
    fun onReading(sensed: Long, delivered: Long)

    /** A frame was swapped; [tookReading] when it is the first to show a new reading. On the render thread. */
    fun onSwapped(drawn: Long, swapped: Long, tookReading: Boolean)

    /** The latest swapped frame was latched for the screen. On the main thread. */
    fun onLatched()
}
