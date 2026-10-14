package io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs

import java.io.File

interface CarFetcher {
    suspend fun fetchCar(contentHash: ByteArray): Result<ByteArray>

    /**
     * Streams the CAR archive to a temporary file instead of buffering it in memory, for archives
     * too large to hold on the heap. The caller owns the returned file and must delete it once the
     * archive has been unpacked.
     *
     * [onProgress] reports download progress as `(downloaded, total)` bytes; `total` is `null` when
     * the gateway did not send a Content-Length.
     */
    suspend fun fetchCarToFile(
        contentHash: ByteArray,
        onProgress: (downloaded: Long, total: Long?) -> Unit
    ): Result<File>
}
