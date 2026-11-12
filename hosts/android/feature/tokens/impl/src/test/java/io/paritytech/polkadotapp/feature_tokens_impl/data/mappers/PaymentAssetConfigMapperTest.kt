package io.paritytech.polkadotapp.feature_tokens_impl.data.mappers

import io.paritytech.polkadotapp.feature_tokens_impl.data.paymentAsset.PaymentAssetConfigRemote
import io.paritytech.polkadotapp.feature_tokens_impl.domain.paymentAsset.PaymentAssetConfig
import org.junit.Assert.assertEquals
import org.junit.Test

private const val SQUARE_URL = "https://cdn.example.com/cash/square.svg"
private const val WIDE_URL = "http://cdn.example.com/cash/wide.png"

class PaymentAssetConfigMapperTest {
    @Test
    fun `should keep the symbol and absolute web urls`() {
        val config = remote(symbol = "CASH", square = SQUARE_URL, wide = WIDE_URL).toDomain()

        assertEquals(PaymentAssetConfig("CASH", SQUARE_URL, WIDE_URL), config)
    }

    @Test
    fun `should trim values and treat blank ones as unpublished`() {
        val config = remote(symbol = " USD ", square = " $SQUARE_URL ", wide = "   ").toDomain()

        assertEquals(PaymentAssetConfig("USD", SQUARE_URL, null), config)
    }

    @Test
    fun `should treat missing fields as unpublished`() {
        val config = remote(symbol = null, square = null, wide = null).toDomain()

        assertEquals(PaymentAssetConfig(null, null, null), config)
    }

    @Test
    fun `should reject logo urls that are not absolute web urls`() {
        assertNoLogos(remote(symbol = null, square = "square.svg", wide = "file:///etc/passwd").toDomain())
        assertNoLogos(remote(symbol = null, square = "ftp://cdn.example.com/square.svg", wide = "https://").toDomain())
        assertNoLogos(remote(symbol = null, square = "not a url", wide = "https://exa mple.com/x.svg").toDomain())
    }

    private fun remote(symbol: String?, square: String?, wide: String?) =
        PaymentAssetConfigRemote(symbol = symbol, iconSquareUrl = square, iconWideUrl = wide)

    private fun assertNoLogos(config: PaymentAssetConfig) {
        assertEquals(PaymentAssetConfig(config.symbol, null, null), config)
    }
}
