package io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor

import android.net.Uri
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.filterResultSuccess
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.needsAttention
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageAccountBackupObserver
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.TotalBalanceUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFaceSource
import io.paritytech.polkadotapp.feature_products_api.domain.product.ProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.UsernameOfAccountUseCase
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.VideoGamesProgressUseCase
import io.paritytech.polkadotapp.feature_wallet_impl.data.config.AppSharingConfigRepository
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.DigitalDollarBalance
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.PocketRank
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.toPocketRank
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class PocketInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val totalBalanceUseCase: TotalBalanceUseCase,
    private val usernameOfAccountUseCase: UsernameOfAccountUseCase,
    private val gamesProgressUseCase: VideoGamesProgressUseCase,
    private val coinageBackupService: CoinageBackupService,
    private val coinageAccountBackupObserver: CoinageAccountBackupObserver,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val pocketCollection: PocketCollection,
    private val pocketFaceSource: PocketFaceSource,
    private val productContentWarmUp: ProductContentWarmUp,
    private val appSharingConfigRepository: AppSharingConfigRepository,
) {
    fun observeBackupProgress(): Flow<BackupProgress> = coinageBackupService.subscribeProgress()

    fun observeAccountBackupPending(): Flow<Boolean> = coinageAccountBackupObserver.subscribeStatus().map { it.needsAttention }

    fun observeAddress(): Flow<String> = flow { emit(getPeopleChainAddress()) }

    private suspend fun getPeopleChainAddress(): String {
        val walletAccount = accountRepository.getWalletAccount()
        val peopleChain = chainRegistry.getChain(knownChains.people)
        return walletAccount.addressIn(peopleChain)
    }

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

    fun observeUsername(): Flow<String> = usernameOfAccountUseCase()
        .filterNotNull()
        .map { it.username.getDisplayUsername() }

    fun observeProductCards(): Flow<List<PocketCard>> = pocketCollection.observeCards()

    /** Fetches the product's pages before its card is opened, so opening it does not wait on them. */
    suspend fun warmUpProduct(key: PocketCardKey): Result<Unit> = productContentWarmUp.warmUp(key.productId)

    /** Collect only while the face is on screen: collecting keeps the backing product's worker running. */
    fun observeFace(key: PocketCardKey): Flow<JsWidget> = pocketFaceSource.observeFace(key)

    // Whether the card was there to remove is the core's concern, not the screen's.
    suspend fun removeProductCard(key: PocketCardKey): Result<Unit> = pocketCollection.removeCard(key).map {}

    fun sendFaceAction(key: PocketCardKey, actionId: String, payload: ByteArray) =
        pocketFaceSource.sendAction(key, actionId, payload)

    suspend fun resolveFaceImage(key: PocketCardKey, source: JsImageSource): Result<Uri> =
        pocketFaceSource.resolveImage(key, source)

    context(scope: ComputationalScope)
    fun observeRank(): Flow<PocketRank> = if (FeatureOption.PERSONHOOD.isEnabled) {
        gamesProgressUseCase.videoGamesProgressFlow().map { it.toPocketRank() }
    } else {
        flowOf(PocketRank.Basic)
    }

    suspend fun getAppSharingUrl(): Result<String> = appSharingConfigRepository.getAppSharingUrl()
}
