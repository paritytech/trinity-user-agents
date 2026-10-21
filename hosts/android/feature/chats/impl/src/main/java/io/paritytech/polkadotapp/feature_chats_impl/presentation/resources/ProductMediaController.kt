package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.webkit.MimeTypeMap
import android.widget.Toast
import androidx.compose.runtime.staticCompositionLocalOf
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.openImage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import timber.log.Timber
import java.io.File
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/** What the media items of product messages do (02-DESIGN §6.2): load the bytes, show an image full screen, open a file. */
interface ProductMediaController {
    suspend fun load(resource: ChatResourceModel, mimeType: String): Result<File>

    /** Shows the image in [file] full screen in the image viewer of the system. */
    fun openImage(file: File)

    /** Lets the user pick an app that opens [file]. */
    fun openFile(file: File, mimeType: String)
}

/** The media items of product messages draw only on the room screen, which gives the controller. */
val LocalProductMediaController = staticCompositionLocalOf<ProductMediaController> {
    error("No ProductMediaController was provided")
}

class RealProductMediaController @Inject constructor(
    private val loader: ChatMediaLoader,
    private val fileProvider: FileProvider,
    @param:ApplicationContext private val appContext: Context,
) : ProductMediaController {
    override suspend fun load(resource: ChatResourceModel, mimeType: String): Result<File> =
        loader.load(resource, MimeTypeMap.getSingleton().getExtensionFromMimeType(mimeType))

    override fun openImage(file: File) {
        appContext.openImage(fileProvider.uriOf(file))
    }

    override fun openFile(file: File, mimeType: String) {
        val view = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(fileProvider.uriOf(file), mimeType)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        val chooser = Intent.createChooser(view, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        try {
            appContext.startActivity(chooser)
        } catch (error: ActivityNotFoundException) {
            Timber.w(error, "No app opens a file of a product message")
            Toast.makeText(appContext, RCommon.string.app_not_found_error, Toast.LENGTH_SHORT).show()
        }
    }
}
