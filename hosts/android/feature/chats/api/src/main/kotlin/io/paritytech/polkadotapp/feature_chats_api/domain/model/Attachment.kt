package io.paritytech.polkadotapp.feature_chats_api.domain.model

import android.content.Context
import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.getResourceImageSize
import io.paritytech.polkadotapp.common.utils.getResourceUri

sealed interface Attachment {
    val uri: Uri?
    val meta: Meta

    /**
     * A local attachment that has not yet been published to HOP. This represents either a file
     * picked by the user to send through chat (before upload completes and identifiers become
     * available), or a built-in resource such as a drawable or a file from scoped app storage.
     */
    data class Embedded(
        override val uri: Uri,
        override val meta: Meta
    ) : Attachment

    /**
     * A remote attachment that has been published to HOP and is ready to be downloaded
     * by the receiving side. Contains the [identifier] and [ticket] required to
     * retrieve the file from the network.
     */
    data class Hosted(
        override val uri: Uri?,
        val identifier: DataByteArray,
        val ticket: HopTicket,
        val nodeUrl: String,
        override val meta: Meta
    ) : Attachment

    sealed interface Meta {
        val mimeType: String
        val size: InformationSize

        data class Image(
            val width: Int,
            val height: Int,
            val blurHash: String?,
            override val mimeType: String,
            override val size: InformationSize
        ) : Meta

        data class Video(
            val duration: Long,
            // BlurHash of the first frame
            val blurHash: String?,
            override val mimeType: String,
            override val size: InformationSize
        ) : Meta

        data class General(val fileName: String, override val mimeType: String, override val size: InformationSize) : Meta
    }

    companion object {
        fun image(uri: Uri, width: Int, height: Int, mimeType: String = "image/*", size: InformationSize = InformationSize.ZERO): Embedded {
            return Embedded(uri = uri, meta = Meta.Image(width, height, blurHash = null, mimeType = mimeType, size = size))
        }

        fun file(uri: Uri, fileName: String, mimeType: String, size: InformationSize): Embedded {
            return Embedded(uri = uri, meta = Meta.General(fileName, mimeType, size))
        }
    }
}

fun Context.createAttachmentFromDrawable(drawableResId: Int): Attachment.Embedded {
    val size = getResourceImageSize(drawableResId)
    return Attachment.image(getResourceUri(drawableResId), size.width, size.height)
}
