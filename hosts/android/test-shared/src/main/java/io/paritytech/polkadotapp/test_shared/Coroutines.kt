package io.paritytech.polkadotapp.test_shared

import kotlinx.coroutines.yield

private const val SETTLE_YIELDS = 5

suspend fun settle() = repeat(SETTLE_YIELDS) { yield() }
