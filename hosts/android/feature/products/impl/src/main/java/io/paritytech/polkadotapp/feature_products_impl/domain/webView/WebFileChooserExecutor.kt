package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.MediaStore
import android.webkit.WebChromeClient.FileChooserParams
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.core.content.ContextCompat
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor
import java.io.File

/**
 * Launches the system file picker for a WebView `<input type="file">` request and maps the
 * result back to the URIs the WebView expects.
 *
 * [createIntent] starts from [FileChooserParams.createIntent], which encodes the accepted MIME
 * types and the multiple-selection flag. For image inputs it additionally offers the camera as a
 * capture option (see [buildCameraIntentOrNull]) so the experience matches iOS, where WKWebView
 * surfaces the camera for image inputs natively.
 *
 * [handleResult] reads the picked URIs from [Intent.getClipData] (multiple selection) or
 * [Intent.getData] (single); `FileChooserParams.parseResult` is deliberately not used because it
 * ignores `ClipData`, so multi-select would be dropped as a cancellation. A camera capture returns
 * neither, so its result is read back from [cameraFile].
 */
class WebFileChooserExecutor(
    private val activity: ComponentActivity,
    private val params: FileChooserParams,
    private val fileProvider: FileProvider,
) : ActivityResultExecutor<Array<Uri>?>(activity) {
    private var cameraFile: File? = null

    override fun createIntent(): Intent {
        val pickerIntent = params.createIntent()
        val cameraIntent = buildCameraIntentOrNull() ?: return pickerIntent
        return Intent.createChooser(pickerIntent, null).apply {
            putExtra(Intent.EXTRA_INITIAL_INTENTS, arrayOf(cameraIntent))
        }
    }

    /**
     * A capture intent for image inputs, or null to leave the chooser file-only.
     *
     * Gated on the CAMERA permission already being granted: the app declares CAMERA in its
     * manifest, so the OS requires it for [MediaStore.ACTION_IMAGE_CAPTURE] — and we deliberately
     * never request it from here, so opening a file picker never triggers a permission prompt
     * (matching how a normal browser treats `<input type="file">`). Users who granted camera
     * elsewhere get the option; everyone else gets the gallery-only chooser, unchanged.
     */
    private fun buildCameraIntentOrNull(): Intent? {
        if (!params.acceptsImages()) return null
        val cameraGranted = ContextCompat.checkSelfPermission(
            activity,
            Manifest.permission.CAMERA,
        ) == PackageManager.PERMISSION_GRANTED
        if (!cameraGranted) return null

        val file = fileProvider.generateTempFile("camera_capture/${System.currentTimeMillis()}.jpg")
        cameraFile = file
        return Intent(MediaStore.ACTION_IMAGE_CAPTURE).apply {
            putExtra(MediaStore.EXTRA_OUTPUT, fileProvider.uriOf(file))
            addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        }
    }

    override fun handleResult(result: ActivityResult): Result<Array<Uri>?> {
        // Anything other than RESULT_OK (cancel, back) yields null, which releases the <input>
        // so a later tap can reopen the picker.
        if (result.resultCode != Activity.RESULT_OK) {
            cameraFile?.delete()
            return Result.success(null)
        }

        val data = result.data
        val clipData = data?.clipData
        val uris = when {
            clipData != null -> {
                cameraFile?.delete()
                Array(clipData.itemCount) { clipData.getItemAt(it).uri }
            }
            data?.data != null -> {
                cameraFile?.delete()
                arrayOf(data.data!!)
            }
            // No picker payload on RESULT_OK means the camera wrote to EXTRA_OUTPUT.
            else -> cameraFile?.let { arrayOf(fileProvider.uriOf(it)) }
        }
        return Result.success(uris)
    }
}

private fun FileChooserParams.acceptsImages(): Boolean {
    val types = acceptTypes.filter { it.isNotBlank() }
    return types.isEmpty() || types.any { it.startsWith("image/") || it == "*/*" }
}
