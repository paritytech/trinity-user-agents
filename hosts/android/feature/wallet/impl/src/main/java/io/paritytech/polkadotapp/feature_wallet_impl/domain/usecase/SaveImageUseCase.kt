package io.paritytech.polkadotapp.feature_wallet_impl.domain.usecase

import android.graphics.Bitmap
import android.net.Uri
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.withContext
import java.util.UUID
import javax.inject.Inject

class SaveImageUseCase @Inject constructor(
    private val fileProvider: FileProvider,
    private val dispatchers: CoroutineDispatchers
) {
    suspend operator fun invoke(image: Bitmap): Uri = withContext(dispatchers.io) {
        val file = fileProvider.generateTempFile("id_share/${UUID.randomUUID()}.jpg")
        file.outputStream().use { stream ->
            image.compress(Bitmap.CompressFormat.JPEG, IMAGE_JPEG_DEFAULT_COMPRESSION, stream)
        }
        fileProvider.uriOf(file)
    }

    companion object {
        const val IMAGE_JPEG_DEFAULT_COMPRESSION = 85
    }
}
