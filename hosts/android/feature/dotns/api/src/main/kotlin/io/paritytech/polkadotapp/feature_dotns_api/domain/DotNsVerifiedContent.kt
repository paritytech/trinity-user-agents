package io.paritytech.polkadotapp.feature_dotns_api.domain

import java.io.File

/**
 * The content of a dotNS name, taken only once its archive checks out (HX4): its root is the name's contenthash and
 * every block hashes to its CID. Content that fails is not kept.
 */
interface DotNsVerifiedContent {
    /**
     * Downloads and checks the content [dotNsName] points at now, keeping it under its contenthash; null when the
     * name points at none. Content checked before is not downloaded again.
     */
    suspend fun fetch(dotNsName: String): Result<VerifiedContent?>

    /** The files of [contentHash] if they were checked and are still kept. */
    fun kept(contentHash: String): VerifiedContent?

    /** Serves the checked [contentHash] for [dotNsName], in place of what its record says. */
    fun serve(dotNsName: String, contentHash: String)
}

/** Checked content: its contenthash, and the root directory of its files. */
class VerifiedContent(val contentHash: String, val directory: File)
