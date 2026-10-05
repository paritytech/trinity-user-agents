package io.parity.truapi

import uniffi.truapi.SecretCoreStorageKey
import uniffi.truapi.secretCoreStorageKeyIdentifier
import java.util.concurrent.ConcurrentHashMap

internal class TestSecrets : HostSecretStorage {
    private val values = ConcurrentHashMap<String, ByteArray>()
    override suspend fun read(key: SecretCoreStorageKey): ByteArray? = values[secretCoreStorageKeyIdentifier(key)]?.copyOf()
    override suspend fun write(key: SecretCoreStorageKey, value: ByteArray) { values[secretCoreStorageKeyIdentifier(key)] = value.copyOf() }
    override suspend fun clear(key: SecretCoreStorageKey) { values.remove(secretCoreStorageKeyIdentifier(key)) }
}

internal object TestWallet : WalletSecretProvider {
    override suspend fun readWalletRootEntropy(walletId: String): ByteArray = ByteArray(32) { (it + 1).toByte() }
}
