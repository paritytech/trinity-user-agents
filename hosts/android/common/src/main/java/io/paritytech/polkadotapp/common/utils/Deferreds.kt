package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.Deferred

suspend operator fun <T> Deferred<T>.invoke() = await()
