package io.paritytech.polkadotapp.common.utils

import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

private val DEFAULT_TICK_DURATION = 1.seconds

fun currentTimestampFlow(interval: Duration = DEFAULT_TICK_DURATION): Flow<Timestamp> {
    return flow {
        while (true) {
            emit(System.currentTimeMillis())
            delay(interval)
        }
    }
}

@Composable
fun rememberCurrentTimeMillisWithDelay(delay: Duration): State<Long> {
    return produceState(initialValue = System.currentTimeMillis(), delay) {
        while (true) {
            delay(delay)
            value = System.currentTimeMillis()
        }
    }
}
