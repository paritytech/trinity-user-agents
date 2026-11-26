package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.utils.KeyedMutex
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.retrying
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.IpfsHttpException
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import java.io.File
import java.io.IOException
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class DotNsContentDownloader @Inject constructor(
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val builds: DotNsBuilds,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val networkStateService: NetworkStateService,
) {
    private val downloadLocks = KeyedMutex<ContentHash>()

    private val unreferencedContentDeletion = Mutex()
    private var unreferencedContentDeleted = false

    suspend fun download(contentHash: ContentHash, retryPolicy: NetworkRetryPolicy, progressDomain: String?): Result<File> {
        deleteUnreferencedContentBeforeFirstDownload()

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
            .flatMap { carFile ->
                runCancellableCatching {
                    progressDomain?.let(progressRegistry::markUnpacking)
                    unpack(contentHash, carFile)
                }
            }
            .requireNotNull { IllegalStateException("nothing unpacked for $contentHash") }
            .onSuccess { progressDomain?.let(progressRegistry::markCompleted) }
            .logFailure("failed to download $contentHash")
    }

    private suspend fun unpack(contentHash: ContentHash, carFile: File): File? {
        try {
            contentStorage.saveContentStreaming(contentHash) { writer ->
                CarParser.unpack(carFile) { path, content -> writer.write(path, content) }
            }
            return contentStorage.getContentDirectory(contentHash)
        } finally {
            carFile.delete()
        }
    }

    private suspend fun deleteUnreferencedContentBeforeFirstDownload() = unreferencedContentDeletion.withLock {
        if (unreferencedContentDeleted) return@withLock
        builds.deleteAllUnreferencedContent()
        unreferencedContentDeleted = true
    }

    private fun isWorthRetrying(error: Throwable): Boolean = when (error) {
        is IpfsHttpException -> error.isTransient
        is IOException -> true
        else -> false
    }
}
