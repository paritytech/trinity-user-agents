package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.paritytech.polkadotapp.common.domain.model.X25519KeyPair
import io.paritytech.polkadotapp.common.domain.model.X25519PrivateKey
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.X25519KeyGenerator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrUAPIOurDeviceKeypairProvider @Inject constructor(
    // Lazy: the runtime's own dependencies reach chat code that asks for this keypair.
    private val runtimeProvider: Lazy<TrUAPIHostRuntimeProvider>,
    private val keyGenerator: X25519KeyGenerator,
) : OurDeviceKeypairProvider {
    private val mutex = Mutex()

    @Volatile
    private var cached: X25519KeyPair? = null

    override suspend fun get(): X25519KeyPair {
        cached?.let { return it }

        return mutex.withLock {
            cached ?: load().also { cached = it }
        }
    }

    override suspend fun publicKey(): X25519PublicKey = get().publicKey

    private suspend fun load(): X25519KeyPair = runtimeProvider.get().runtime()
        .mapCatching { runtime ->
            keyGenerator.createKeyPair(X25519PrivateKey.fromDerivedBytes(runtime.deviceEncryptionKey()))
        }
        .getOrThrow()
}
