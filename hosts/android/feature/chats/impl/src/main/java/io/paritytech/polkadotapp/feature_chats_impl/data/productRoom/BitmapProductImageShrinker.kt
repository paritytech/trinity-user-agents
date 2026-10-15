package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import java.io.ByteArrayOutputStream
import javax.inject.Inject

/** Encodes an image again as JPEG, at a lower quality and then at a smaller size, until it fits (as iOS). */
class BitmapProductImageShrinker @Inject constructor() : ProductImageShrinker {
    override fun shrink(image: ByteArray, limitBytes: Int): ShrunkImage? {
        val original = BitmapFactory.decodeByteArray(image, 0, image.size) ?: return null
        var candidate = original

        repeat(SCALE_STEPS) {
            JPEG_QUALITIES.forEach { quality ->
                val bytes = ByteArrayOutputStream().use { output ->
                    candidate.compress(Bitmap.CompressFormat.JPEG, quality, output)
                    output.toByteArray()
                }
                if (bytes.size <= limitBytes) return ShrunkImage(bytes, JPEG_MIME_TYPE, candidate.width, candidate.height)
            }

            val width = (candidate.width * SCALE_FACTOR).toInt().coerceAtLeast(1)
            val height = (candidate.height * SCALE_FACTOR).toInt().coerceAtLeast(1)
            candidate = Bitmap.createScaledBitmap(candidate, width, height, true)
        }

        return null
    }

    private companion object {
        const val SCALE_STEPS = 6
        const val SCALE_FACTOR = 0.6f
        const val JPEG_MIME_TYPE = "image/jpeg"
        val JPEG_QUALITIES = listOf(85, 70, 50, 35)
    }
}
