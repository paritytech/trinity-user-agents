package io.paritytech.polkadotapp.feature_products_impl.data.bundled

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProductFiles
import java.io.InputStream
import javax.inject.Inject
import javax.inject.Singleton

/** The app's assets. Debug, nightly and safetynet builds only; a build made without the bundle ships none. */
@Singleton
class AssetBundledProductFiles @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : BundledProductFiles {
    override val enabled: Boolean = BuildConfig.BUNDLED_PRODUCTS_ENABLED

    // Listed once, so the core asking whether a path is there never opens an asset.
    private val shipped: Set<String> by lazy { if (enabled) filesUnder(BundledProductFiles.ROOT).toSet() else emptySet() }

    override fun list(dir: String): List<String> =
        runCatching { context.assets.list(dir)?.toList() }.getOrNull().orEmpty()

    override fun exists(path: String): Boolean = path in shipped

    override fun readText(path: String): String? = open(path)?.use { it.readBytes().decodeToString() }

    override fun open(path: String): InputStream? = runCatching { context.assets.open(path) }.getOrNull()

    // The asset manager lists a directory's children and nothing under a file.
    private fun filesUnder(path: String): List<String> {
        val children = list(path)
        return if (children.isEmpty()) listOf(path) else children.flatMap { filesUnder("$path/$it") }
    }
}
