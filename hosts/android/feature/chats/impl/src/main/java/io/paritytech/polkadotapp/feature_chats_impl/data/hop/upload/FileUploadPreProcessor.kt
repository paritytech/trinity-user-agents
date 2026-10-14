package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import androidx.exifinterface.media.ExifInterface
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import javax.inject.Inject

interface FileUploadPreProcessor {
    fun preProcess(file: ByteArray, mimeType: String): ByteArray
}

class CompressImages @Inject constructor() : FileUploadPreProcessor {
    override fun preProcess(file: ByteArray, mimeType: String): ByteArray {
        if (!mimeType.startsWith("image/") || mimeType == "image/gif") return file

        val decoded = BitmapFactory.decodeByteArray(file, 0, file.size) ?: return file
        val bitmap = applyExifRotation(decoded, file)
        val output = ByteArrayOutputStream()
        bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, output)

        if (bitmap !== decoded) bitmap.recycle()
        decoded.recycle()

        return output.toByteArray()
    }

    private fun applyExifRotation(bitmap: Bitmap, file: ByteArray): Bitmap {
        val orientation = runCatching {
            ExifInterface(ByteArrayInputStream(file)).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
        }.getOrDefault(ExifInterface.ORIENTATION_NORMAL)

        val matrix = when (orientation) {
            ExifInterface.ORIENTATION_ROTATE_90 -> Matrix().apply { postRotate(90f) }
            ExifInterface.ORIENTATION_ROTATE_180 -> Matrix().apply { postRotate(180f) }
            ExifInterface.ORIENTATION_ROTATE_270 -> Matrix().apply { postRotate(270f) }
            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> Matrix().apply { postScale(-1f, 1f) }
            ExifInterface.ORIENTATION_FLIP_VERTICAL -> Matrix().apply { postScale(1f, -1f) }
            ExifInterface.ORIENTATION_TRANSPOSE -> Matrix().apply { postRotate(90f); postScale(-1f, 1f) }
            ExifInterface.ORIENTATION_TRANSVERSE -> Matrix().apply { postRotate(270f); postScale(-1f, 1f) }
            else -> return bitmap
        }

        return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
    }

    companion object {
        private const val JPEG_QUALITY = 85
    }
}
