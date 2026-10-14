package io.paritytech.polkadotapp.common.domain.model

import kotlin.time.Duration

// In millis
typealias Timestamp = Long

fun Duration.toTimestamp(): Timestamp {
    return inWholeMilliseconds
}
