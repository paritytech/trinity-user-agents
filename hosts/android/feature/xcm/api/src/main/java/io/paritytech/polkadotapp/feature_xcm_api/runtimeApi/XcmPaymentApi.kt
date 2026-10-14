package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi

import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.ScaleResultError
import io.paritytech.polkadotapp.chains.network.binding.toResult
import io.paritytech.polkadotapp.common.utils.flatMap
import timber.log.Timber

fun <T> Result<ScaleResult<T, *>>.getInnerSuccessOrThrow(): T {
    return getOrThrow()
        .toResult()
        .onFailure {
            Timber.e("Xcm api call failed: ${(it as ScaleResultError).content}")
        }.getOrThrow()
}

fun <T> Result<ScaleResult<T, *>>.flatten(): Result<T> {
    return flatMap { it.toResult() }
}
