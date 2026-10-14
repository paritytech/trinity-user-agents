package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.tools_car_parser.FileContent
import io.paritytech.polkadotapp.tools_car_parser.FilePath
import java.io.File
import java.io.InputStream

typealias ContentHash = HexString

/** Sink the streaming CAR unpack pushes each reassembled file into. */
fun interface CarContentWriter {
    fun write(path: FilePath, content: InputStream)
}
interface DotNsContentStorage {
    suspend fun saveContent(contentHash: ContentHash, files: Map<FilePath, FileContent>)

    /**
     * Streams an unpacked archive to disk: runs [write] (which drives the CAR unpack, pushing each
     * file into the supplied [CarContentWriter]) and applies the same single-root-file-vs-directory
     * layout as [saveContent]. If [write] throws, any partially-written content is removed before
     * the exception propagates, so [getContentDirectory] never returns a half-unpacked archive.
     */
    suspend fun saveContentStreaming(contentHash: ContentHash, write: (CarContentWriter) -> Unit)

    fun getContentDirectory(contentHash: ContentHash): File?

    fun contentExists(contentHash: ContentHash): Boolean

    suspend fun deleteContent(contentHash: ContentHash)

    suspend fun deleteAll()
}
