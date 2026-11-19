package io.paritytech.polkadotapp.feature_dotns_api.domain

import uniffi.truapi.ArchiveReader
import java.io.InputStream

/** The files of an archive, read by the core's archive rules and streamed to the WebView. */
interface ArchiveFiles : ArchiveReader {
    fun open(path: String): InputStream?
}
