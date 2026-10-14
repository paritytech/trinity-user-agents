package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.resolveToLocalFile
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import java.io.File
import javax.inject.Inject

/** Files inside a product's worker archive, as dotNS serves it locally. */
class ProductWorkerArchive @Inject constructor(
    private val dotNsResolver: DotNsResolver,
) {
    suspend fun file(productId: ProductId, path: String): Result<File> {
        val workerHost = ProductManifest.hostOf(productId, ExecutableKind.WORKER)
        return dotNsResolver.resolveToLocalFile(workerHost.value).mapCatching { archive -> archive.fileInside(path) }
    }

    private fun File.fileInside(path: String): File {
        val file = File(this, path)
        require(file.canonicalPath.startsWith(canonicalPath + File.separator)) { "path escapes the archive" }
        require(file.isFile) { "'$path' is not in the archive" }
        return file
    }
}
