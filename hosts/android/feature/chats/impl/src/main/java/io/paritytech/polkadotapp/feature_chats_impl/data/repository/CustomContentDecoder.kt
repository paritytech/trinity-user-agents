package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRendererId
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderersById
import kotlinx.serialization.KSerializer

interface CustomContentDecoder {
    fun decode(rendererId: CustomChatMessageRendererId, content: ByteArray): Result<Any?>

    fun encode(rendererId: CustomChatMessageRendererId, value: Any?): Result<ByteArray>
}

class MatchRendererCustomContentDecoder(
    private val knownRenderers: CustomChatMessageRenderersById
) : CustomContentDecoder {
    override fun decode(rendererId: CustomChatMessageRendererId, content: ByteArray): Result<Any?> {
        return findRenderer(rendererId).mapCatching {
            BinaryScale.decodeFromByteArray(it.contentSerializer, content)
        }
    }

    override fun encode(rendererId: CustomChatMessageRendererId, value: Any?): Result<ByteArray> {
        return findRenderer(rendererId).mapCatching {
            @Suppress("UNCHECKED_CAST")
            BinaryScale.encodeToByteArray(it.contentSerializer as KSerializer<Any?>, value)
        }
    }

    private fun findRenderer(rendererId: CustomChatMessageRendererId): Result<CustomChatMessageRenderer<*>> {
        val renderer = knownRenderers[rendererId]
        return if (renderer != null) {
            Result.success(renderer)
        } else {
            Result.failure(IllegalArgumentException("Unknown renderer id: $renderer"))
        }
    }
}

class AlwaysFailCustomContentDecoder : CustomContentDecoder {
    override fun decode(rendererId: CustomChatMessageRendererId, content: ByteArray): Result<Any?> {
        return Result.failure(IllegalStateException("AlwaysFailCustomContentDecoder"))
    }

    override fun encode(rendererId: CustomChatMessageRendererId, value: Any?): Result<ByteArray> {
        return Result.failure(IllegalStateException("AlwaysFailCustomContentDecoder"))
    }
}
