package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.utils

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.activity.result.ActivityResult
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.ActivityResultExecutor
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import java.io.File
import javax.inject.Inject

data class AttachmentResult(
    val uri: Uri,
    val mimeType: String,
    val size: InformationSize
)

class AttachmentExecutor @Inject constructor(
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider
) : ActivityResultExecutor<AttachmentResult?>(contextManager.requireActivity()) {
    private var cameraFile: File? = null
    private var cameraAllowed: Boolean = true

    override fun createIntent(): Intent {
        val getContentIntent = Intent(Intent.ACTION_GET_CONTENT).apply {
            type = "*/*"
            putExtra(Intent.EXTRA_MIME_TYPES, arrayOf("image/*", "video/*"))
            addCategory(Intent.CATEGORY_OPENABLE)
        }

        val cameraIntent = if (cameraAllowed) {
            val file = fileProvider.generateTempFile("camera_capture/${System.currentTimeMillis()}.jpg")
            cameraFile = file

            Intent(MediaStore.ACTION_IMAGE_CAPTURE).apply {
                putExtra(MediaStore.EXTRA_OUTPUT, fileProvider.uriOf(file))
                addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            }
        } else {
            null
        }

        return Intent.createChooser(getContentIntent, null).apply {
            if (cameraIntent != null) {
                putExtra(Intent.EXTRA_INITIAL_INTENTS, arrayOf(cameraIntent))
            }
        }
    }

    override fun handleResult(result: ActivityResult): Result<AttachmentResult?> {
        if (result.resultCode != Activity.RESULT_OK) {
            cameraFile?.delete()
            return Result.success(null)
        }

        val pickedFromGallery = result.data?.data
        if (pickedFromGallery != null) {
            cameraFile?.delete()
        }

        val uri = pickedFromGallery ?: cameraFile?.let(fileProvider::uriOf)
            ?: return Result.failure(IllegalStateException("No file URI received"))

        val contentResolver = contextManager.applicationContext.contentResolver
        val mimeType = contentResolver.getType(uri) ?: "application/octet-stream"
        val fileSize = contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val sizeIndex = cursor.getColumnIndex(OpenableColumns.SIZE)
            cursor.moveToFirst()
            cursor.getLong(sizeIndex)
        } ?: 0L

        return Result.success(AttachmentResult(uri = uri, mimeType = mimeType, size = fileSize.bytes))
    }

    // TODO: temporary quick solution for camera permissions
    // later we are going to rework it with in-app custom picker anyway
    fun setCameraAllowed(isAllowed: Boolean) {
        cameraAllowed = isAllowed
    }
}
