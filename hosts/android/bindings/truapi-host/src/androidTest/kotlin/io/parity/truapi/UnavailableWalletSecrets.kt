package io.parity.truapi

import uniffi.truapi.HostRejection
import uniffi.truapi.NativeWalletSecretProvider

object UnavailableWalletSecrets : NativeWalletSecretProvider {
    override suspend fun readWalletRootEntropy(walletId: String): ByteArray =
        throw HostRejection.Rejected("No wallet in this test")
}
