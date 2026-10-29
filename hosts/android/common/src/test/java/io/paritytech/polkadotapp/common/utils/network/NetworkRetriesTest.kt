package io.paritytech.polkadotapp.common.utils.network

import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.IOException

class NetworkRetriesTest {
    private val online = MutableStateFlow(true)
    private val networkStateService: NetworkStateService = mockk {
        every { isNetworkAvailable } returns online
    }

    // A phone in a tunnel would otherwise burn every attempt before it is out.
    @Test
    fun `a failure while offline is not counted and the read resumes once the network is back`() = runTest {
        online.value = false
        launch {
            delay(5_000)
            online.value = true
        }
        val attemptTimes = mutableListOf<Long>()

        networkStateService.retrying(NetworkRetryPolicy.Attempts(1), isRetryable = { true }) {
            attemptTimes += currentTime
            Result.failure<String>(IOException("gateway down"))
        }

        assertEquals(listOf(0L, 5_000L), attemptTimes)
    }
}
