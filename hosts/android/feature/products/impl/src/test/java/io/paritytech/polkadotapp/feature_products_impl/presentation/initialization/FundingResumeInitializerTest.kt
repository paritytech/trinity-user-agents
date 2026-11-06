package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test

class FundingResumeInitializerTest {
    private val inProgress = MutableStateFlow<Boolean?>(null)
    private val runtime = mockk<FundingRuntime> {
        every { inProgress } returns this@FundingResumeInitializerTest.inProgress
        coEvery { start() } returns Result.success(Unit)
    }
    private val preferences = mockk<Preferences>(relaxed = true)

    @Test
    fun `starts the core at launch when a session was in progress as the app last ran`() = runTest {
        every { preferences.getBoolean(IN_PROGRESS, false) } returns true

        initialize()

        coVerify(exactly = 1) { runtime.start() }
    }

    // Nothing to resume, so the core waits for a screen or product to start it.
    @Test
    fun `leaves the core alone at launch when nothing was in progress`() = runTest {
        every { preferences.getBoolean(IN_PROGRESS, false) } returns false

        initialize()

        coVerify(exactly = 0) { runtime.start() }
    }

    @Test
    fun `remembers what the core last pushed, not before it pushed anything`() = runTest {
        every { preferences.getBoolean(IN_PROGRESS, false) } returns false
        initialize()
        verify(exactly = 0) { preferences.putBoolean(IN_PROGRESS, any()) }

        inProgress.value = true
        runCurrent()
        inProgress.value = false
        runCurrent()

        verify(exactly = 1) { preferences.putBoolean(IN_PROGRESS, true) }
        verify(exactly = 1) { preferences.putBoolean(IN_PROGRESS, false) }
    }

    private fun TestScope.initialize() {
        val scope = object : ComputationalScope, CoroutineScope by backgroundScope {}
        with(scope) { FundingResumeInitializer(runtime, preferences).initialize() }
        runCurrent()
    }

    private companion object {
        const val IN_PROGRESS = "funding_in_progress"
    }
}
