package io.paritytech.polkadotapp.feature_dotns_api.presentation

import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import io.paritytech.polkadotapp.feature_dotns_api.domain.ArchiveFiles
import uniffi.truapi.archiveFile
import java.io.ByteArrayInputStream
import java.io.File
import java.io.InputStream

/**
 * Answers WebView requests from an archive by the core's rules ([archiveFile]), the same for a published archive and a
 * product the app ships. [mainDocumentResponseHeaders] go on main-frame documents only.
 */
class ArchiveContent(
    private val files: ArchiveFiles,
    private val mainDocumentResponseHeaders: Map<String, String>,
) {
    fun respond(root: String, request: WebResourceRequest): WebResourceResponse {
        val file = archiveFile(root, request.url.path.orEmpty(), request.isForMainFrame, files)
        val stream = file?.let { files.open(it.path) } ?: return notFound()
        return if (request.isForMainFrame && mainDocumentResponseHeaders.isNotEmpty()) {
            WebResourceResponse(file.mime, "UTF-8", 200, "OK", mainDocumentResponseHeaders, stream)
        } else {
            WebResourceResponse(file.mime, "UTF-8", stream)
        }
    }

    private fun notFound() = WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream("Not Found".toByteArray())).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }
}

/** Published archives unpacked on disk, by absolute path. */
object DiskArchiveFiles : ArchiveFiles {
    override fun list(dir: String): List<String> = File(dir).list()?.toList().orEmpty()

    override fun exists(path: String): Boolean = File(path).isFile

    override fun readText(path: String): String? = File(path).takeIf(File::isFile)?.readText()

    override fun open(path: String): InputStream? = File(path).takeIf(File::isFile)?.inputStream()
}
