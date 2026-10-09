package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import android.net.Uri
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.design.components.qr.QrCodeBitmapGenerator
import io.paritytech.polkadotapp.feature_wallet_impl.domain.usecase.SaveImageUseCase
import kotlinx.coroutines.withContext
import javax.inject.Inject

class IdShareImageRenderer @Inject constructor(
    private val qrCodeBitmapGenerator: QrCodeBitmapGenerator,
    private val saveImageUseCase: SaveImageUseCase,
    private val dispatchers: CoroutineDispatchers
) {
    suspend fun render(address: String): Result<Uri> = runCancellableCatching {
        val qr = withContext(dispatchers.computation) { qrCodeBitmapGenerator.generate(address) }
        saveImageUseCase(qr)
    }
}
