package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.retrying
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.IpfsHttpException
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import timber.log.Timber
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class DotNsContentDownloader @Inject constructor(
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val networkStateService: NetworkStateService,
) {
    private val downloadLocks = KeyedMutex<ContentHash>()

    suspend fun download(contentHash: ContentHash, retryPolicy: NetworkRetryPolicy, progressDomain: String?): Result<File> {
        return downloadLocks.withLock(contentHash) {
            contentStorage.getContentDirectory(contentHash)?.let { directory ->
                progressDomain?.let(progressRegistry::markCompleted)
                return@withLock Result.success(directory)
            }

            fetchAndUnpack(contentHash, retryPolicy, progressDomain)
        }
    }

    private suspend fun fetchAndUnpack(contentHash: ContentHash, retryPolicy: NetworkRetryPolicy, progressDomain: String?): Result<File> {
        Timber.d("content $contentHash not on the device, fetching CAR")
        progressDomain?.let(progressRegistry::markResolving)

        return networkStateService.retrying(retryPolicy, ::isWorthRetrying) {
            carFetcher.fetchCarToFile(
                contentHash = contentHash.fromHex(),
                onProgress = { downloaded, total -> progressDomain?.let { progressRegistry.markDownloadProgress(it, downloaded, total) } }
            )
        }
            .mapCatching { carFile ->
                progressDomain?.let(progressRegistry::markUnpacking)
                unpack(contentHash, carFile)
            }
            .onSuccess { progressDomain?.let(progressRegistry::markCompleted) }
            .logFailure("failed to download $contentHash")
    }

    private suspend fun unpack(contentHash: ContentHash, carFile: File): File {
        try {
            contentStorage.saveContentStreaming(contentHash) { writer ->
                CarParser.unpack(carFile) { path, content -> writer.write(path, content) }
            }
            return requireNotNull(contentStorage.getContentDirectory(contentHash)) { "nothing unpacked for $contentHash" }
        } finally {
            carFile.delete()
        }
    }

    private fun isWorthRetrying(error: Throwable): Boolean = when (error) {
        is IpfsHttpException -> error.isTransient
        is IllegalArgumentException -> false
        else -> true
    }
}
