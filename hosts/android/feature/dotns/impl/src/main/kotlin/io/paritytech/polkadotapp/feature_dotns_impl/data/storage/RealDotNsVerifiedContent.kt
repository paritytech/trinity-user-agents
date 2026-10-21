package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import androidx.core.content.edit
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsVerifiedContent
import io.paritytech.polkadotapp.feature_dotns_api.domain.VerifiedContent
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.tools_car_parser.CarParser
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import javax.inject.Inject

class RealDotNsVerifiedContent @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val contractApi: DotNsContractApi,
    private val carFetcher: CarFetcher,
    private val contentStorage: DotNsContentStorage,
    private val contentHashOverrides: ContentHashOverrides,
) : DotNsVerifiedContent {
    // The contenthashes whose archives passed the check; content kept under another hash may not have.
    private val checked by lazy { context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE) }

    override suspend fun fetch(dotNsName: String): Result<VerifiedContent?> =
        contractApi.resolveContentHash(dotNsName).flatMap { contentHash ->
            if (contentHash == null) Result.success(null) else fetchChecked(contentHash)
        }

    override fun kept(contentHash: String): VerifiedContent? {
        if (!checked.getBoolean(contentHash, false)) return null
        return contentStorage.getContentDirectory(contentHash)?.let { VerifiedContent(contentHash, it) }
    }

    override fun serve(dotNsName: String, contentHash: String) {
        contentHashOverrides.putContentHashOverride(dotNsName, contentHash)
    }

    private suspend fun fetchChecked(contentHash: ByteArray): Result<VerifiedContent> {
        val hex = contentHash.toHexString()
        kept(hex)?.let { return Result.success(it) }
        return runCancellableCatching { Cid.cast(contentHash) }
            .flatMap { root -> carFetcher.fetchCarToFile(contentHash, onProgress = { _, _ -> }).map { root to it } }
            .flatMap { (root, carFile) ->
                runCancellableCatching {
                    try {
                        CarParser.verify(carFile, root)
                        contentStorage.deleteContent(hex)
                        contentStorage.saveContentStreaming(hex) { writer ->
                            CarParser.unpack(carFile) { path, content -> writer.write(path, content) }
                        }
                    } finally {
                        carFile.delete()
                    }
                    checked.edit { putBoolean(hex, true) }
                    VerifiedContent(hex, checkNotNull(contentStorage.getContentDirectory(hex)) { "content $hex was not kept" })
                }
            }
    }

    private companion object {
        const val PREFS_NAME = "dotns_verified_content"
    }
}
