package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class TrUAPIForegroundReporterTest {
    @Test
    fun `each change of the app state goes to the core one time`() = runBlocking {
        val reports = mutableListOf<Boolean>()
        var returns = 0

        flowOf(
            AppLifecycleState.FOREGROUND,
            AppLifecycleState.FOREGROUND,
            AppLifecycleState.BACKGROUND,
            AppLifecycleState.FOREGROUND,
        ).reportForeground(setAppForeground = { reports += it }, onForeground = { returns++ })

        assertEquals(listOf(true, false, true), reports)
        assertEquals(2, returns)
    }

    // A failed report must not stop the reports that come after it.
    @Test
    fun `a report that the core refuses does not stop the next one`() = runBlocking {
        val reports = mutableListOf<Boolean>()

        flowOf(AppLifecycleState.BACKGROUND, AppLifecycleState.FOREGROUND).reportForeground(
            setAppForeground = {
                reports += it
                check(it) { "the core is closed" }
            },
            onForeground = {},
        )

        assertEquals(listOf(false, true), reports)
    }
}
