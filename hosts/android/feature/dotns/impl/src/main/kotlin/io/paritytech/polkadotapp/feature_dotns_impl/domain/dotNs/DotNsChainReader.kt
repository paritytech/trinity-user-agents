package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.network.binding.DispatchError
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.retrying
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import java.util.concurrent.TimeoutException
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

internal class DotNsChainReader @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val contentHashOverrides: ContentHashOverrides,
    private val networkStateService: NetworkStateService,
) {
    suspend fun readContentHash(dotNsName: String, retryPolicy: NetworkRetryPolicy): Result<ContentHash> {
        Timber.d("resolving content hash for $dotNsName")

        val override = contentHashOverrides.getContentHashOverride(dotNsName)
        if (override != null) return Result.success(override)

        return read(retryPolicy) { contractApi.resolveContentHash(dotNsName) }
            .mapNotNull { it.toHexString() }
            .logFailure("failed to resolve content hash for $dotNsName")
            .requireNotNull { IllegalStateException("Domain $dotNsName is not registered or has no content") }
    }

    suspend fun readRecord(dotNsName: String, key: String, retryPolicy: NetworkRetryPolicy): Result<String?> {
        return read(retryPolicy) { contractApi.getMetadata(dotNsName, key) }
    }

    private suspend fun <T> read(retryPolicy: NetworkRetryPolicy, request: suspend () -> Result<T>): Result<T> {
        return networkStateService.retrying(retryPolicy, ::isWorthRetrying) {
            withTimeoutOrNull(READ_TIMEOUT) { request() }
                ?: Result.failure(TimeoutException("Chain did not answer within $READ_TIMEOUT"))
        }
    }

    private fun isWorthRetrying(error: Throwable): Boolean = when (error) {
        is ReviveContractReverted, is DispatchError, is IllegalArgumentException -> false
        else -> true
    }

    private companion object {
        val READ_TIMEOUT = 30.seconds
    }
}
