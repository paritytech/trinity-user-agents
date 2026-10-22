package io.paritytech.polkadotapp.feature_dotns_api.domain

/**
 * Content the app ships for a dotNS name, served in place of that name's on-chain contenthash with no
 * download. For a product the host carries itself, such as the chat product, until a verified on-chain
 * release replaces it.
 */
interface DotNsBundledContent {
    /**
     * Serves [files] (paths relative to the archive root) for [dotNsName] under [contentHash], a hex id of the
     * content that changes whenever the files do (the resolver decodes it as hex). Idempotent: content already
     * stored is not written again.
     */
    suspend fun install(dotNsName: String, contentHash: String, files: Map<String, ByteArray>)
}
