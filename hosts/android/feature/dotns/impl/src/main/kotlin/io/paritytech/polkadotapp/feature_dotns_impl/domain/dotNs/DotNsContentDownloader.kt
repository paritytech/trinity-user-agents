package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.retrying
import io.paritytech.polkadotapp.common.utils.requireNotNull
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.IpfsHttpException
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import timber.log.Timber
import java.io.File
import javax.inject.Inject

internal class DotNsContentDownloader @Inject constructor(
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val progressRegistry: DotNsLoadProgressRegistry,
    private val networkStateService: NetworkStateService,
) {
    suspend fun download(contentHash: ContentHash, retryPolicy: NetworkRetryPolicy, progressDomain: String): Result<File> {
        contentStorage.getContentDirectory(contentHash)?.let { directory ->
            progressRegistry.markCompleted(progressDomain)
            return Result.success(directory)
        }

        Timber.d("content $contentHash not on the device, fetching CAR")
        progressRegistry.markResolving(progressDomain)

        return networkStateService.retrying(retryPolicy, ::isWorthRetrying) {
            carFetcher.fetchCarToFile(
                contentHash = contentHash.fromHex(),
                onProgress = { downloaded, total -> progressRegistry.markDownloadProgress(progressDomain, downloaded, total) }
            )
        }
            .mapCatching { carFile ->
                progressRegistry.markUnpacking(progressDomain)
                unpack(contentHash, carFile)
            }
            .requireNotNull { IllegalStateException("nothing unpacked for $contentHash") }
            .onSuccess { progressRegistry.markCompleted(progressDomain) }
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

    private fun isWorthRetrying(error: Throwable): Boolean = when (error) {
        is IpfsHttpException -> error.isTransient
        is IllegalArgumentException -> false
        else -> true
    }
}
