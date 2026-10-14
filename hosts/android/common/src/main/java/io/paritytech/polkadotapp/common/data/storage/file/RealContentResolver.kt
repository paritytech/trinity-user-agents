package io.paritytech.polkadotapp.common.data.storage.file

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Size
import androidx.exifinterface.media.ExifInterface
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.InputStream
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

internal class RealContentResolver @Inject constructor(
    @param:ApplicationContext private val context: Context
) : ContentResolver {
    override fun openInputStream(uri: Uri): InputStream? {
        return context.contentResolver.openInputStream(uri)
    }

    override fun queryFileName(uri: Uri): String? {
        return context.contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                if (index >= 0) cursor.getString(index) else null
            } else null
        }
    }

    override fun getImageSize(uri: Uri): Size {
        val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }

        context.contentResolver.openInputStream(uri)?.use {
            BitmapFactory.decodeStream(it, null, options)
        }

        val width = options.outWidth.coerceAtLeast(0)
        val height = options.outHeight.coerceAtLeast(0)

        val swapped = isExifRotated(uri)
        return if (swapped) Size(height, width) else Size(width, height)
    }

    private fun isExifRotated(uri: Uri): Boolean {
        return runCatching {
            context.contentResolver.openInputStream(uri)?.use { stream ->
                val orientation = ExifInterface(stream)
                    .getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)
                orientation == ExifInterface.ORIENTATION_ROTATE_90 ||
                    orientation == ExifInterface.ORIENTATION_ROTATE_270 ||
                    orientation == ExifInterface.ORIENTATION_TRANSPOSE ||
                    orientation == ExifInterface.ORIENTATION_TRANSVERSE
            } ?: false
        }.getOrDefault(false)
    }

    override fun getVideoDuration(uri: Uri): Duration? {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(context, uri)
            retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)
                ?.toLongOrNull()
                ?.milliseconds
        } finally {
            retriever.release()
        }
    }

    override fun loadImagePreview(uri: Uri, targetSize: Int): Result<Bitmap> = runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }

        val longestSide = maxOf(bounds.outWidth, bounds.outHeight)
        check(longestSide > 0) { "Failed to read image bounds for $uri" }

        // BitmapFactory rounds inSampleSize down to the nearest power of two internally.
        val options = BitmapFactory.Options().apply { inSampleSize = maxOf(1, longestSide / targetSize) }
        val bitmap = context.contentResolver.openInputStream(uri)
            ?.use { BitmapFactory.decodeStream(it, null, options) }
        checkNotNull(bitmap) { "Failed to decode image preview for $uri" }
    }

    override fun loadVideoPreview(uri: Uri, targetSize: Int): Result<Bitmap> = runCatching {
        val retriever = MediaMetadataRetriever()
        try {
            retriever.setDataSource(context, uri)
            val frame = retriever.getScaledFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC, targetSize, targetSize)
            checkNotNull(frame) { "Failed to extract video preview frame for $uri" }
        } finally {
            retriever.release()
        }
    }
}
