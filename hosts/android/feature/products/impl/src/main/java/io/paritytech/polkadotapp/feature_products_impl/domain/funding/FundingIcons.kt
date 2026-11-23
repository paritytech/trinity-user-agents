package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.net.Uri
import coil.ComponentRegistry
import coil.ImageLoader
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
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

private const val FUNDING_ICON_SCHEME = "funding-icon"

/**
 * The image model for [id]'s icon as [provider] ships it, resolved only once it is drawn; `null` when the id names
 * no icon.
 */
fun fundingIconModel(provider: ProductId, kind: FundingIconKind, id: String): String? =
    FundingIconPath.iconName(id)?.let { "$FUNDING_ICON_SCHEME:${provider.value}/${kind.directory}/$it" }

/**
 * Loads a [fundingIconModel] from the provider's worker archive on disk. The provider is read from the image model,
 * which anything can build, so it is validated against the TLD. A miss fails the load and the screen keeps the
 * monogram.
 */
internal class FundingIconFetcher(
    private val model: String,
    private val dotNsTldProvider: DotNsTldProvider,
    private val files: FundingIconFiles,
    private val options: Options,
    private val imageLoader: ImageLoader,
) : Fetcher {
    override suspend fun fetch(): FetchResult? {
        val file = dotNsTldProvider.getTld()
            .flatMap { tld -> fileOf(tld) }
            // Coil reads a failed load from a throw; the screen keeps the monogram.
            .getOrThrow()
        val components = imageLoader.components

        return components.newFetcher(components.map(file, options), options, imageLoader)?.first?.fetch()
    }

    private suspend fun fileOf(tld: DotNsTld): Result<File> {
        val parts = model.split('/')
        val kind = FundingIconKind.entries.firstOrNull { it.directory == parts.getOrNull(1) }
        if (parts.size != 3 || kind == null) return Result.failure(IllegalArgumentException("'$model' is not a funding icon"))

        return productIdFromHost(parts[0], tld).flatMap { provider -> files.file(provider, kind, parts[2]) }
    }

    class Factory(
        private val dotNsTldProvider: DotNsTldProvider,
        private val files: FundingIconFiles,
    ) : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? {
            if (data.scheme != FUNDING_ICON_SCHEME) return null

            return FundingIconFetcher(data.schemeSpecificPart, dotNsTldProvider, files, options, imageLoader)
        }
    }
}

internal class FundingIconImageLoaderRegistrar @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
    private val files: FundingIconFiles,
) : ImageLoaderComponentRegistrar {
    override fun register(builder: ComponentRegistry.Builder) {
        builder.add(FundingIconFetcher.Factory(dotNsTldProvider, files))
    }
}
