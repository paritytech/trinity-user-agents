package io.paritytech.polkadotapp.feature_chats_impl.data.attachment

import android.graphics.Bitmap
import com.vanniktech.blurhash.BlurHash

internal fun Bitmap.encodeBlurHash(): Result<String> = runCatching {
    BlurHash.encode(this, componentX = BLURHASH_COMPONENT_X, componentY = BLURHASH_COMPONENT_Y)
}

internal const val BLURHASH_PREVIEW_TARGET_PX = 128
internal const val BLURHASH_COMPONENT_X = 4
internal const val BLURHASH_COMPONENT_Y = 3
