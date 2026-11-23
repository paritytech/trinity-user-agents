package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.drawable.BitmapDrawable
import coil.ImageLoader
import coil.decode.DecodeResult
import coil.decode.Decoder
import coil.fetch.SourceResult
import coil.imageLoader
import coil.request.CachePolicy
import coil.request.ErrorResult
import coil.request.ImageRequest
import coil.request.Options
import coil.request.SuccessResult
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_api.model.toJsImageSource
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.truapi.ImageSource
import uniffi.truapi.MaterialFailure
import uniffi.truapi.checkMaterialImage
import javax.inject.Inject

/** The core refused a material image argument for [reason]. */
class MaterialImageRefused(val reason: MaterialFailure) : Exception("material image refused: $reason")

/**
 * Loads the images a material reads. Unlike an Image node's source, an image argument keeps its straight
 * alpha and is checked by the core.
 */
class MaterialImages @Inject constructor(@ApplicationContext private val context: Context) {
    /** Loads [source] through [resolver]; a refused image fails with [MaterialImageRefused]. */
    suspend fun load(resolver: JsImageResolver, source: ImageSource): Result<Bitmap> =
        runCancellableCatching {
            when (source) {
                is ImageSource.Inline -> withContext(Dispatchers.Default) { decodeStraightAlpha(source.v1) }
                is ImageSource.Bulletin, is ImageSource.Archive -> fetch(resolver, source)
            }
        }

    private suspend fun fetch(resolver: JsImageResolver, source: ImageSource): Bitmap {
        val model = source.toJsImageSource()?.let { resolver.resolve(it) }
            ?: throw MaterialImageRefused(MaterialFailure.NOT_FOUND)
        val request = ImageRequest.Builder(context)
            .data(model)
            .allowHardware(false)
            .memoryCachePolicy(CachePolicy.DISABLED)
            .decoderFactory(CheckedDecoder.Factory())
            .build()
        return when (val result = context.imageLoader.execute(request)) {
            is SuccessResult -> (result.drawable as BitmapDrawable).bitmap
            is ErrorResult -> throw result.throwable
        }
    }

    private class CheckedDecoder(private val result: SourceResult, private val options: Options) : Decoder {
        override suspend fun decode(): DecodeResult {
            val bytes = result.source.source().use { it.readByteArray() }
            checkMaterialImage(bytes)?.let { throw MaterialImageRefused(it) }
            return DecodeResult(BitmapDrawable(options.context.resources, decodeStraightAlpha(bytes)), isSampled = false)
        }

        class Factory : Decoder.Factory {
            override fun create(result: SourceResult, options: Options, imageLoader: ImageLoader): Decoder =
                CheckedDecoder(result, options)
        }
    }
}

private fun decodeStraightAlpha(bytes: ByteArray): Bitmap {
    val options = BitmapFactory.Options().apply {
        inPremultiplied = false
        inScaled = false
    }
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
        ?: throw MaterialImageRefused(MaterialFailure.UNSUPPORTED)
}
