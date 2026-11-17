package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import java.io.File

/**
 * Serves every product's pages from [archive] on the `dot` network.
 *
 * Plain fakes rather than MockK: on a device, a mocked suspend function returning `Result` hands back the boxed
 * `Result`, so the client fails with a ClassCastException and the page never loads.
 */
internal fun archiveContent(archive: File): DotNsWebViewClient {
    val tld = requireNotNull(DotNsTld.parse("dot"))
    val tldProvider = object : DotNsTldProvider {
        override fun currentTldOrNull(): DotNsTld = tld

        override suspend fun getTld(): Result<DotNsTld> = Result.success(tld)
    }
    val resolver = object : DotNsResolver {
        override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> = Result.success(Uri.fromFile(archive))

        override suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?> = Result.success(null)

        override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> = flowOf(DotNsLoadProgress.Idle)

        override suspend fun clearCache() = Unit
    }
    return DotNsWebViewClient(resolver, tldProvider)
}
