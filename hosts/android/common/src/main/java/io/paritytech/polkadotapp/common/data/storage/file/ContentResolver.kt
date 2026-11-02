package io.paritytech.polkadotapp.common.data.storage.file

import android.graphics.Bitmap
import android.net.Uri
import android.util.Size
import io.paritytech.polkadotapp.common.utils.InformationSize
import java.io.InputStream
import kotlin.time.Duration

interface ContentResolver {
    fun openInputStream(uri: Uri): InputStream?

    fun queryFileName(uri: Uri): String?

    /** Size of the content behind [uri], or null when it can't be opened. */
    fun querySize(uri: Uri): InformationSize?

    fun getImageSize(uri: Uri): Size

    fun getVideoDuration(uri: Uri): Duration?

    /** Decodes an image preview no larger than [targetSize] on the longest side. */
    fun loadImagePreview(uri: Uri, targetSize: Int): Result<Bitmap>

    /** Extracts a video preview frame no larger than [targetSize] on the longest side. */
    fun loadVideoPreview(uri: Uri, targetSize: Int): Result<Bitmap>
}
