package io.paritytech.polkadotapp.feature_products_impl.domain.derivation

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class ReservedProductAccountDerivationProviderTest {
    private val tldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(DotNsTld.parse("paseo")!!)
    }

    /**
     * RFC-0022 / individuality-apps#1639: the wallet's main account is the personhood product's
     * account 0, `//product//peopl.<tld>/index_bytes(0)`, so the `peopl` product owns the
     * person's dotNS usernames. The retired `uid` product must not be derived anywhere.
     */
    @Test
    fun `wallet account is the personhood product account 0`() = runBlocking<Unit> {
        val provider = ReservedProductAccountDerivationProvider(tldProvider, ReservedProductIds::personhood)

        assertEquals(
            "//product//peopl.paseo/${DerivationIndex32.default().asPathSegment()}",
            provider.provideDerivationPath(),
        )
    }
}
