package io.paritytech.polkadotapp.feature_dotns_api.domain

import uniffi.truapi.BundleReader
import java.io.InputStream

/** Files an archive is served from: a published archive on disk, or the products the app ships. */
interface ArchiveFiles : BundleReader {
    fun open(path: String): InputStream?
}
