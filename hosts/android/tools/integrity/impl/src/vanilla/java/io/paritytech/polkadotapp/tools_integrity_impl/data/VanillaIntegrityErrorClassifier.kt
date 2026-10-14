package io.paritytech.polkadotapp.tools_integrity_impl.data

import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import kotlinx.coroutines.CancellationException
import java.security.InvalidAlgorithmParameterException
import java.security.KeyStoreException
import java.security.ProviderException

internal fun Throwable.toIntegrityError(): IntegrityError = when (this) {
    is CancellationException -> throw this
    is IntegrityError -> this

    // AndroidKeyStore refuses to mint an attestation chain on devices without hardware
    // attestation, or when the requested security level is claimed but absent.
    is ProviderException,
    is InvalidAlgorithmParameterException,
    is KeyStoreException,
    is UnsupportedOperationException -> IntegrityError.AttestationUnavailable

    else -> IntegrityError.Unknown
}
