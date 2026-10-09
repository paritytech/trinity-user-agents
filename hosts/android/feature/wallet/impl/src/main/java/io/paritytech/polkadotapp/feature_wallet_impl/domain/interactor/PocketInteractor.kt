package io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor

import android.net.Uri
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.utils.filterResultSuccess
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.needsAttention
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardFaceOnOpen
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFaceSource
import io.paritytech.polkadotapp.feature_products_api.domain.product.ProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.DigitalDollarBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class PocketInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    private val coinageBackupService: CoinageBackupService,
    private val coinageAccountBackupObserver: CoinageAccountBackupObserver,
    private val pocketCollection: PocketCollection,
    private val pocketFaceSource: PocketFaceSource,
    private val pocketCardFaceOnOpen: PocketCardFaceOnOpen,
    private val productContentWarmUp: ProductContentWarmUp,
) {
    fun observeBackupProgress(): Flow<BackupProgress> = coinageBackupService.subscribeProgress()

    fun observeAccountBackupPending(): Flow<Boolean> = coinageAccountBackupObserver.subscribeStatus().map { it.needsAttention }

    fun observeDigitalDollarBalance(): Flow<DigitalDollarBalance> = flow {
        val asset = chainAssetProvider.asset()
        emitAll(
            totalBalanceUseCase.subscribeTotalBalance()
                .logFailure("PocketInteractor: Failed to get coinage balance")
                .filterResultSuccess()
                .filterNotNull()
                .map { balance ->
                    DigitalDollarBalance(
                        total = asset.withAmount(balance.total),
                        ready = asset.withAmount(balance.availablePrivate)
                    )
                }
        )
    }

    fun observeProductCards(): Flow<List<PocketCard>> = pocketCollection.observeCards()

    /** Fetches the product's pages before its card is opened, so opening it does not wait on them. */
    suspend fun warmUpProduct(key: PocketCardKey): Result<Unit> = productContentWarmUp.warmUp(key.productId)

    suspend fun faceShownOnOpen(key: PocketCardKey): Boolean = pocketCardFaceOnOpen.faceShownOnOpen(key)

    /** Collect only while the face is on screen: collecting keeps the backing product's worker running. */
    fun observeFace(key: PocketCardKey): Flow<JsWidget> = pocketFaceSource.observeFace(key)

    // Whether the card was there to remove is the core's concern, not the screen's.
    suspend fun removeProductCard(key: PocketCardKey): Result<Unit> = pocketCollection.removeCard(key).map {}

    fun sendFaceAction(key: PocketCardKey, actionId: String, payload: ByteArray) =
        pocketFaceSource.sendAction(key, actionId, payload)

    suspend fun resolveFaceImage(key: PocketCardKey, source: JsImageSource): Result<Uri> =
        pocketFaceSource.resolveImage(key, source)
}
