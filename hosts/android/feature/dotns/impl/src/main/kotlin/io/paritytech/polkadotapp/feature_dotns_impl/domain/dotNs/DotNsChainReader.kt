package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapNotNull
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import timber.log.Timber
import javax.inject.Inject

internal class DotNsChainReader @Inject constructor(
    private val contractApi: DotNsContractApi,
    private val contentHashOverrides: ContentHashOverrides,
) {
    suspend fun readContentHash(dotNsName: String): Result<ContentHash> {
        Timber.d("resolving content hash for $dotNsName")

        val override = contentHashOverrides.getContentHashOverride(dotNsName)
        if (override != null) return Result.success(override)

        return contractApi.resolveContentHash(dotNsName)
            .mapNotNull { it.toHexString() }
            .logFailure("failed to resolve content hash for $dotNsName")
            .requireNotNull { IllegalStateException("Domain $dotNsName is not registered or has no content") }
    }

    suspend fun readRecord(dotNsName: String, key: String): Result<String?> {
        return contractApi.getMetadata(dotNsName, key)
    }
}
