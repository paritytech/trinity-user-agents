package io.paritytech.polkadotapp.feature_dotns_api.domain

import uniffi.truapi.BundleReader
import java.io.InputStream

/** The files of an archive, read by the core's archive rules and streamed to the WebView. */
interface ArchiveFiles : BundleReader {
    fun open(path: String): InputStream?
}
