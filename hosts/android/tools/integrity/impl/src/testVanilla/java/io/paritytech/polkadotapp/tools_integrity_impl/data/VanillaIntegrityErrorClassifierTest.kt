package io.paritytech.polkadotapp.tools_integrity_impl.data

import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import org.junit.Assert.assertEquals
import org.junit.Test
import java.security.InvalidAlgorithmParameterException
import java.security.KeyStoreException
import java.security.ProviderException

class VanillaIntegrityErrorClassifierTest {

    @Test
    fun `keystore refusals mean the device cannot attest`() {
        val keystoreFailures = listOf(
            ProviderException("no attestation support"),
            InvalidAlgorithmParameterException("bad spec"),
            KeyStoreException("no chain"),
            UnsupportedOperationException("strongbox missing"),
        )

        keystoreFailures.forEach { failure ->
            assertEquals(
                "${failure::class.simpleName} should be AttestationUnavailable",
                IntegrityError.AttestationUnavailable,
                failure.toIntegrityError()
            )
        }
    }

    @Test
    fun `already typed errors pass through`() {
        assertEquals(
            IntegrityError.AttestationRejected,
            IntegrityError.AttestationRejected.toIntegrityError()
        )
    }

    @Test
    fun `unrecognised throwable is unknown`() {
        assertEquals(IntegrityError.Unknown, IllegalStateException("boom").toIntegrityError())
    }
}
