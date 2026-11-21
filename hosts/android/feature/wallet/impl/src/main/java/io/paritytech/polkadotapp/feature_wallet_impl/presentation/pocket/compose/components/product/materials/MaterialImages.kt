package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

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
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_api.model.toJsImageSource
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.truapi.ImageSource
import uniffi.truapi.MaterialFailure
import uniffi.truapi.checkMaterialImage

internal class MaterialImageRefused(val reason: MaterialFailure) : Exception("material image refused: $reason")

// Unlike an Image node's source, an image argument keeps its straight alpha and is checked by the core.
internal object MaterialImages {
    suspend fun load(context: Context, resolver: JsImageResolver, source: ImageSource): Result<Bitmap> =
        runCancellableCatching {
            when (source) {
                is ImageSource.Inline -> withContext(Dispatchers.Default) { decode(source.v1) }
                is ImageSource.Bulletin, is ImageSource.Archive -> fetch(context, resolver, source)
            }
        }

    private suspend fun fetch(context: Context, resolver: JsImageResolver, source: ImageSource): Bitmap {
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

    private fun decode(bytes: ByteArray): Bitmap {
        val options = BitmapFactory.Options().apply {
            inPremultiplied = false
            inScaled = false
        }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)
            ?: throw MaterialImageRefused(MaterialFailure.UNSUPPORTED)
    }

    private class CheckedDecoder(private val result: SourceResult, private val options: Options) : Decoder {
        override suspend fun decode(): DecodeResult {
            val bytes = result.source.source().use { it.readByteArray() }
            checkMaterialImage(bytes)?.let { throw MaterialImageRefused(it) }
            return DecodeResult(BitmapDrawable(options.context.resources, decode(bytes)), isSampled = false)
        }

        class Factory : Decoder.Factory {
            override fun create(result: SourceResult, options: Options, imageLoader: ImageLoader): Decoder =
                CheckedDecoder(result, options)
        }
    }
}
