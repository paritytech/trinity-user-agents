package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.CoroutineDispatcher
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.Dispatchers as BuiltInDispatchers

interface CoroutineDispatchers {
    val main: CoroutineDispatcher
    val io: CoroutineDispatcher
    val computation: CoroutineDispatcher
}

@Singleton
class RealCoroutineDispatchers @Inject constructor() : CoroutineDispatchers {
    override val main: CoroutineDispatcher = BuiltInDispatchers.Main
    override val io: CoroutineDispatcher = BuiltInDispatchers.IO
    override val computation: CoroutineDispatcher = BuiltInDispatchers.Default
}
