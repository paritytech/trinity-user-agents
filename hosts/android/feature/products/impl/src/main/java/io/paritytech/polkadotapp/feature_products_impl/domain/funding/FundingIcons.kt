package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductWorkerArchive
import uniffi.truapi.ProductId
import java.io.File
import java.util.Locale
import javax.inject.Inject

/** Which of a provider's funding icons is asked for, by the directory the provider ships them in. */
enum class FundingIconKind(val directory: String) {
    NETWORK("networks"),
    TOKEN("tokens"),
}

/**
 * Where a provider ships its funding icons in its worker archive: `funding/icons/networks/<network>.png` and
 * `funding/icons/tokens/<symbol>.png`, the id lowercased. Each is a square 96×96 PNG with transparency.
 */
object FundingIconPath {
    /** `null` for an id that is empty or, lowercased, leaves `[a-z0-9-]`: no provider ships an icon for it. */
    fun of(kind: FundingIconKind, id: String): String? {
        val name = iconName(id) ?: return null

        return "funding/icons/${kind.directory}/$name.png"
    }

    internal fun iconName(id: String): String? = id.lowercase(Locale.ROOT).takeIf(ICON_NAME::matches)

    private val ICON_NAME = Regex("[a-z0-9-]+")
}

/** A network's or token's icon file in [provider]'s worker archive, never in another provider's. */
class FundingIconFiles @Inject constructor(
    private val archive: ProductWorkerArchive,
) {
    suspend fun file(provider: ProductId, kind: FundingIconKind, id: String): Result<File> {
        val path = FundingIconPath.of(kind, id)
            ?: return Result.failure(IllegalArgumentException("'$id' names no funding icon"))

        return archive.file(provider, path)
    }
}
