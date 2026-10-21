package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsBundledContent
import javax.inject.Inject

class RealDotNsBundledContent @Inject constructor(
    private val contentStorage: DotNsContentStorage,
    private val contentHashOverrides: ContentHashOverrides,
) : DotNsBundledContent {
    override suspend fun install(dotNsName: String, contentHash: String, files: Map<String, ByteArray>) {
        // Checked on every install: clearing the dotNS cache deletes the files but keeps the override.
        if (!contentStorage.contentExists(contentHash)) contentStorage.saveContent(contentHash, files)
        if (contentHashOverrides.getContentHashOverride(dotNsName) != contentHash) {
            contentHashOverrides.putContentHashOverride(dotNsName, contentHash)
        }
    }
}
