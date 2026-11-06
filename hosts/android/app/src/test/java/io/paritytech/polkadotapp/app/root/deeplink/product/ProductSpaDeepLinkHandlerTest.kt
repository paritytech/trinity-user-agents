package io.paritytech.polkadotapp.app.root.deeplink.product

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostCacheHits
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenMarks
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenSource
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class ProductSpaDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private var now = LINK_HANDLED_AT
    private val pageOpens = HostOpenMarks({ now }, { LINK_HANDLED_AT_EPOCH }, HostEventCapture { }, { "link-session" }, HostCacheHits())
    private val router = mockk<RootRouter>(relaxed = true)

    private val handler = ProductSpaDeepLinkHandler(
        coroutineDispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        accountRepository = mockk { coEvery { getWalletAccount() } returns mockk() },
        rootRouter = router,
        dotNsTldProvider = object : DotNsTldProvider {
            override fun currentTldOrNull(): DotNsTld = tld

            override suspend fun getTld(): Result<DotNsTld> = Result.success(tld)
        },
        gate = mockk(),
        pageOpens = pageOpens,
    )

    @Before
    fun parseUris() {
        mockkStatic(Uri::class)
        every { Uri.parse(any()) } answers { uri(firstArg()) }
    }

    @After
    fun restoreUri() = unmockkStatic(Uri::class)

    @Test
    fun `the product page a link opens is timed from when the link was handled`() = runBlocking<Unit> {
        val outcome = handle("https://humanity.dot/") as DeeplinkProcessingOutcome.Navigate
        now = LINK_HANDLED_AT + 300

        outcome.navigate()

        val link = pageOpens.consume("humanity.dot")
        assertEquals(
            Triple(LINK_HANDLED_AT, LINK_HANDLED_AT_EPOCH, HostOpenSource.DEEPLINK),
            Triple(link?.atMillis, link?.atEpochMillis, link?.source),
        )
        verify { router.openSpaBrowser(any()) }
    }

    @Test
    fun `a link that is handled but never navigated to marks no page`() = runBlocking<Unit> {
        handle("https://humanity.dot/")

        assertNull(pageOpens.consume("humanity.dot"))
    }

    private suspend fun handle(link: String) =
        coroutineScope { with(ComputationalScope(this)) { handler.handle(uri(link)) } }.getOrThrow()

    private fun uri(value: String): Uri = mockk {
        every { host } returns value.removePrefix("https://").substringBefore('/')
        every { this@mockk.toString() } returns value
    }

    private companion object {
        const val LINK_HANDLED_AT = 9_000L
        const val LINK_HANDLED_AT_EPOCH = 1_793_880_000_000L
    }
}
