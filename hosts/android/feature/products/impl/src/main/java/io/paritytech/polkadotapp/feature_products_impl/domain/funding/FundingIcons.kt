package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import uniffi.truapi.ProductId
import java.io.File
import java.util.Locale
import javax.inject.Inject

enum class FundingIconKind(val directory: String) {
    NETWORK("networks"),
    TOKEN("tokens"),
}

object FundingIconPath {
    fun of(kind: FundingIconKind, id: String): String? {
        val name = iconName(id) ?: return null

        return "funding/icons/${kind.directory}/$name.png"
    }

    internal fun iconName(id: String): String? = id.lowercase(Locale.ROOT).takeIf(ICON_NAME::matches)

    private val ICON_NAME = Regex("[a-z0-9-]+")
}

// Never another provider's file.
class FundingIconFiles @Inject constructor(
    private val archive: ProductWorkerArchive,
) {
    suspend fun file(provider: ProductId, kind: FundingIconKind, id: String): Result<File> {
        val path = FundingIconPath.of(kind, id)
            ?: return Result.failure(IllegalArgumentException("'$id' names no funding icon"))

        return archive.file(provider, path)
    }
}
