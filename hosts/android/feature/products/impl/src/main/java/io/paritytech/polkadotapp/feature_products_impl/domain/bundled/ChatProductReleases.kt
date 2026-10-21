package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsVerifiedContent
import io.paritytech.polkadotapp.feature_dotns_api.domain.VerifiedContent
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.data.manifest.ManifestParser
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductManifest
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Which release of the chat product runs (HX4): the bundled one, or a newer one from dotNS that passed every check.
 * Its archive is the content of the worker's contenthash with every block matching its CID, its version is above the
 * bundled one, and it has the worker's entry file. The last release that passed is kept and runs while dotNS cannot be
 * read or a newer release fails; the bundled one runs otherwise.
 */
@Singleton
internal class ChatProductReleases @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val bundled: BundledChatProduct,
    private val dotNsResolver: DotNsResolver,
    private val verifiedContent: DotNsVerifiedContent,
    private val manifestParser: ManifestParser,
) {
    private val preferences by lazy { context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE) }

    private class Release(val content: VerifiedContent, val worker: ProductExecutable.Worker)

    private val mutex = Mutex()

    suspend fun resolved(tld: DotNsTld): ResolvedProduct = mutex.withLock {
        bundled.install(tld)
        val host = ProductManifest.hostOf(ReservedProductIds.chat(tld), ExecutableKind.WORKER)
        val floor = bundled.worker(tld)
        val release = newerRelease(host, floor.appVersion) ?: lastRelease(host, floor.appVersion)
            ?: return@withLock bundled.resolved(tld, floor)
        verifiedContent.serve(host.value, release.content.contentHash)
        bundled.resolved(tld, release.worker)
    }

    private suspend fun newerRelease(host: ExecutableHost, floor: SemVer): Release? {
        val record = dotNsResolver.getMetadataEntry(host.value, ProductManifest.EXECUTABLE_RECORD_KEY)
            .logFailure("The chat product's release record was not read")
            .getOrNull() ?: return null
        val worker = workerOf(record, host)?.takeIf { it.appVersion > floor } ?: return null
        val content = verifiedContent.fetch(host.value)
            .logFailure("The chat product's release ${worker.appVersion} failed its check")
            .getOrNull() ?: return null
        if (!content.hasEntry(worker)) {
            Timber.w("The chat product's release %s has no worker entry file", worker.appVersion)
            return null
        }
        preferences.edit().putString(KEY_RECORD, record).putString(KEY_CONTENT, content.contentHash).apply()
        return Release(content, worker)
    }

    private fun lastRelease(host: ExecutableHost, floor: SemVer): Release? {
        val record = preferences.getString(KEY_RECORD, null) ?: return null
        val content = preferences.getString(KEY_CONTENT, null)?.let(verifiedContent::kept) ?: return null
        val worker = workerOf(record, host)?.takeIf { it.appVersion > floor && content.hasEntry(it) } ?: return null
        return Release(content, worker)
    }

    private fun workerOf(record: String, host: ExecutableHost): ProductExecutable.Worker? =
        manifestParser.parseExecutable(record, ExecutableKind.WORKER, host).getOrNull() as? ProductExecutable.Worker

    private fun VerifiedContent.hasEntry(worker: ProductExecutable.Worker): Boolean =
        File(directory, worker.scriptUrl.substringAfter("://").substringAfter('/')).isFile

    private operator fun SemVer.compareTo(other: SemVer): Int =
        compareValuesBy(this, other, SemVer::major, SemVer::minor, SemVer::patch)

    private companion object {
        const val PREFS_NAME = "chat_product_release"
        const val KEY_RECORD = "record"
        const val KEY_CONTENT = "content"
    }
}
