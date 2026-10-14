package io.paritytech.polkadotapp.common.utils

import timber.log.Timber
import kotlin.time.measureTimedValue

inline fun <R> measureExecution(label: String, function: () -> R): R {
    val (value, time) = measureTimedValue(function)
    Timber.tag("Performance").d("$label took $time")

    return value
}
