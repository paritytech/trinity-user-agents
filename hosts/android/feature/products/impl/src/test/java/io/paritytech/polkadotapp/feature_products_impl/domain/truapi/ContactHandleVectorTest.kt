package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.novasama.substrate_sdk_android.extensions.toHexString
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The core mints contact handles and this host recomputes them to answer a
 * lookup, so the two must hash identically. Pinned to the vector the core's
 * own test and the platform docs quote.
 */
class ContactHandleVectorTest {
    @Test
    fun `a handle is BLAKE2b-256 keyed with the handle key over the account`() {
        val handle = contactHandle(
            handleKey = ByteArray(32) { 0x11 },
            account = ByteArray(32) { 0x22 },
        )

        assertEquals(
            "d48c96fce9805f689b0bfa602feacdf3c7770d27e76c25d980eff0955e3714d2",
            handle.toHexString(withPrefix = false),
        )
    }
}
