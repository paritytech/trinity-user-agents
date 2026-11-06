package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostOpenSource
import io.paritytech.polkadotapp.common.data.telemetry.HostPocketTaps
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardOpenRequests
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplink
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplinkAction
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class PocketDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private val humanity = ProductId.parse("humanity.dot").getOrThrow()
    private val key = PocketCardKey(humanity, PocketCardId("humanity"))
    private var now = LINK_HANDLED_AT
    private val pageOpens = HostPocketTaps({ now }, { LINK_HANDLED_AT_EPOCH }, HostEventCapture { })
    private val link = mockk<Uri> { every { this@mockk.toString() } returns "polkadotapp://pocket/open" }

    private val handler = PocketDeepLinkHandler(
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        dotNsTldProvider = mockk {
            every { currentTldOrNull() } returns tld
            coEvery { getTld() } returns Result.success(tld)
        },
        parser = mockk {
            every { parse(any()) } returns PocketDeeplink.Card(humanity, PocketDeeplinkAction.OPEN, "humanity", "")
        },
        publishedCards = mockk(),
        collection = mockk {
            every { observeCards() } returns flowOf(listOf(PocketCard(key = key, title = "Humanity", privileged = true)))
        },
        cardOpenRequests = PocketCardOpenRequests(),
        gate = mockk(),
        accountRepository = mockk(),
        router = mockk(relaxed = true),
        pageOpens = pageOpens,
        context = mockk(),
    )

    @Test
    fun `a held card a link opens is timed from when the link was handled, as a deeplink`() = runBlocking<Unit> {
        val outcome = coroutineScope { with(ComputationalScope(this)) { handler.handle(link) } }.getOrThrow()
        now = LINK_HANDLED_AT + 250

        (outcome as DeeplinkProcessingOutcome.Navigate).navigate()

        val marked = pageOpens.consume(humanity.value)
        assertEquals(
            Triple(LINK_HANDLED_AT, LINK_HANDLED_AT_EPOCH, HostOpenSource.DEEPLINK),
            Triple(marked?.atMillis, marked?.atEpochMillis, marked?.source),
        )
    }

    private companion object {
        const val LINK_HANDLED_AT = 7_000L
        const val LINK_HANDLED_AT_EPOCH = 1_793_880_000_000L
    }
}
