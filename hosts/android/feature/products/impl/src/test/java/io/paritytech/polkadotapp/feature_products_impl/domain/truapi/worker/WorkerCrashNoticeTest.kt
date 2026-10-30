package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBranding
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class WorkerCrashNoticeTest {
    private val shown = mutableListOf<AppNotification>()
    private val notifier = mockk<AppNotifier> { every { notify(any()) } answers { shown += firstArg<AppNotification>() } }
    private val contextManager = mockk<ContextManager> {
        every { applicationContext.getString(RCommon.string.worker_crashing_notice, *anyVararg()) } answers {
            "${secondArg<Array<Any>>()[0]} stopped working. Retrying…"
        }
    }
    private val branding = mockk<FundingProviderBranding>()

    @Test
    fun `the notice names the product as the sheet does`() = runTest {
        coEvery { branding.brand("ramp.dot") } returns FundingProviderBrand("ramp.dot", "Ramp", iconUrl = null)

        notice().onCrashing("ramp.dot")
        advanceUntilIdle()

        assertEquals(listOf(AppNotification.Error("Ramp stopped working. Retrying…")), shown)
    }

    @Test
    fun `a name that cannot be resolved falls back to the product id`() = runTest {
        coEvery { branding.brand("ramp.dot") } throws IllegalStateException("no manifest")

        notice().onCrashing("ramp.dot")
        advanceUntilIdle()

        assertEquals(listOf(AppNotification.Error("ramp.dot stopped working. Retrying…")), shown)
    }

    private fun TestScope.notice() = WorkerCrashNotice({ branding }, notifier, contextManager, testDispatchers())
}
