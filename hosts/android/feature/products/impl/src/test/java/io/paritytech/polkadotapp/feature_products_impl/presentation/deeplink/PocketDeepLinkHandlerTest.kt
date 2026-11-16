package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.content.Context
import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplink
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.PocketDeeplinkParser
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

class PocketDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("paseo"))
    private val link = "polkadotapp://pocket"

    // The TLD saved on an earlier launch, with the People chain unreachable.
    private val tldProvider = mockk<DotNsTldProvider> {
        every { currentTldOrNull() } returns tld
        coEvery { getTld() } returns Result.failure(IOException("people chain unreachable"))
    }

    private val handler = PocketDeepLinkHandler(
        dispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        dotNsTldProvider = tldProvider,
        parser = mockk<PocketDeeplinkParser> { every { parse(link, tld) } returns PocketDeeplink.Collection },
        publishedCards = mockk(),
        collection = mockk(),
        cardOpenRequests = mockk(),
        gate = mockk(),
        accountRepository = mockk<AccountRepository>(relaxed = true),
        router = mockk(),
        context = mockk<Context>(),
    )

    @Test
    fun `an offline cold start opens the collection under the TLD the link was claimed with`() = runBlocking {
        val outcome = handle(mockk<Uri> { every { this@mockk.toString() } returns link })

        assertTrue(outcome.getOrNull() is DeeplinkProcessingOutcome.Navigate)
        coVerify(exactly = 0) { tldProvider.getTld() }
    }

    private suspend fun handle(data: Uri) = coroutineScope { with(ComputationalScope(this)) { handler.handle(data) } }
}
