package io.paritytech.polkadotapp.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups.NavigationHostCalls
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerTransport
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NavigationHostCallsTest {
    // A chat card opens its product by name (`navigateTo("dim2.paseo")`). Read as a URI that name has
    // no host, so no deeplink handler would take it.
    @Test
    fun aBareProductNameNavigatesToThatProduct() {
        assertEquals("https://dim2.paseo", navigatedTo("dim2.paseo"))
    }

    @Test
    fun aDestinationWithASchemeIsHandedOverUnchanged() {
        assertEquals(
            listOf("https://dim2.paseo/nux", "polkadot://dim2.paseo"),
            listOf("https://dim2.paseo/nux", "polkadot://dim2.paseo").map(::navigatedTo),
        )
    }

    // Only a name on the network's TLD is a product; a relative path or another name stays as written.
    @Test
    fun aDestinationThatNamesNoProductIsHandedOverUnchanged() {
        assertEquals(listOf("/nux", "example.com"), listOf("/nux", "example.com").map(::navigatedTo))
    }

    private fun navigatedTo(destination: String): String = runBlocking {
        val navigated = Channel<String>(Channel.UNLIMITED)
        val incoming = slot<(String) -> Unit>()
        val transport = mockk<ContainerTransport> {
            every { registerIncomingHandler(capture(incoming)) } returns Unit
            coEvery { evaluateJs(any()) } returns Unit
        }
        val bridge = ContainerBridge(transport, this, Gson())
        NavigationHostCalls(
            navigationPolicy = NavigationPolicy.DeeplinkNavigation { navigated.trySend(it.toString()) },
            callingProductIdProvider = { Result.success(ProductId.fromStoredValue("dim2.paseo")) },
            dotNsTldProvider = PaseoTld,
        ).registerOn(bridge)

        incoming.captured("""{"type":"request","id":"r1","method":"navigateTo","params":{"destination":"$destination"}}""")

        withTimeout(5_000) { navigated.receive() }
    }

    // Hand-written: on ART mockk hands back the boxed Result of a suspend stub, not its value.
    private object PaseoTld : DotNsTldProvider {
        private val paseo = DotNsTld.parse("paseo")!!

        override fun currentTldOrNull() = paseo

        override suspend fun getTld() = Result.success(paseo)
    }
}
