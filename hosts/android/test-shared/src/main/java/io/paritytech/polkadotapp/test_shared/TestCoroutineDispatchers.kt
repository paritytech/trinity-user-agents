package io.paritytech.polkadotapp.test_shared

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope

/** Every dispatcher is the same one, so nothing a test drives can escape onto a real thread pool. */
class TestCoroutineDispatchers(dispatcher: CoroutineDispatcher) : CoroutineDispatchers {
    override val main: CoroutineDispatcher = dispatcher
    override val io: CoroutineDispatcher = dispatcher
    override val computation: CoroutineDispatcher = dispatcher
}

/**
 * Dispatchers backed by this scope's scheduler.
 *
 * Code under test that switches context keeps running on the test's own clock, so virtual time still
 * advances and `runTest` still waits for it. Handing it a dispatcher from anywhere else would let the work
 * drift onto a real thread and finish after the test has ended.
 */
fun TestScope.testDispatchers(): CoroutineDispatchers =
    TestCoroutineDispatchers(StandardTestDispatcher(testScheduler))
