package io.paritytech.polkadotapp.tools_media_connection_impl.signaling

import io.paritytech.polkadotapp.common.utils.InformationSize

class SendBufferLimits(
    private val pauseAbove: InformationSize,
    private val resumeBelow: InformationSize,
) {
    init {
        require(resumeBelow < pauseAbove) { "resumeBelow must be below pauseAbove" }
    }

    fun shouldWait(bufferedAmount: InformationSize, messageSize: InformationSize, wasThrottled: Boolean): Boolean {
        // An oversized message never fits under pauseAbove: isolate it by waiting for an empty buffer.
        if (messageSize > pauseAbove) return bufferedAmount > InformationSize.ZERO

        // Once throttled, wait for resumeBelow so sends don't pause and resume around pauseAbove.
        if (wasThrottled && bufferedAmount > resumeBelow) return true

        return bufferedAmount + messageSize > pauseAbove
    }
}
