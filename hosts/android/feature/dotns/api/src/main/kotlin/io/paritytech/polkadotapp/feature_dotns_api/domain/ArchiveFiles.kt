package io.paritytech.polkadotapp.feature_dotns_api.domain

import java.io.InputStream

/** The files of an archive, streamed to the WebView in the order the core lists them. */
interface ArchiveFiles {
    /** The regular file at [path], or null for a directory or nothing. */
    fun open(path: String): InputStream?
}
