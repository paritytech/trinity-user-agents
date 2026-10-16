package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.net.URI
import javax.inject.Inject

/**
 * The worker files that a person installs by hand for a product, as on iOS (VERBOSITY-WASM-PRODUCT A2).
 *
 * The folder of a product is `files/Products/<product id>/ChatExtension/`. It holds the entry file of the worker and,
 * as an option, a `manifest.json` that declares the `keepAlive` of the worker. The worker URL of the product names
 * the entry file with a `file:` URL, for example from the debug menu or from the e2e hook. Only a file in the
 * folder of a product can be a worker, because only adb on a debuggable build can put a file there.
 */
class HandInstalledWorkers(private val root: File) {
    @Inject
    constructor(@ApplicationContext context: Context) : this(File(context.filesDir, ROOT_DIRECTORY))

    /** The entry file that [scriptUrl] names, or null when [scriptUrl] is not a `file:` URL in the root folder. */
    fun entryFile(scriptUrl: String): File? {
        val uri = runCatching { URI(scriptUrl) }.getOrNull() ?: return null
        if (uri.scheme != FILE_SCHEME || uri.path == null) return null
        val file = File(uri.path).canonicalFile
        val inside = file.path.startsWith(root.canonicalPath + File.separator)
        return file.takeIf { inside && it.isFile }
    }

    /** The text of the `manifest.json` beside [entry], or null when there is none. */
    fun manifestBeside(entry: File): String? =
        File(entry.parentFile, MANIFEST_FILE).takeIf { it.isFile }?.readText()

    companion object {
        const val ROOT_DIRECTORY = "Products"
        const val MANIFEST_FILE = "manifest.json"
        private const val FILE_SCHEME = "file"
    }
}
