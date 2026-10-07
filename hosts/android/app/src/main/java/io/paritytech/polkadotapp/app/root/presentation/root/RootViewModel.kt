package io.paritytech.polkadotapp.app.root.presentation.root

import android.net.Uri
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.app.root.domain.RootInteractor
import io.paritytech.polkadotapp.app.root.presentation.main.BottomNavHeightProvider
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.flatten
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.OneShotEventChannel
import io.paritytech.polkadotapp.common.utils.disable
import io.paritytech.polkadotapp.common.utils.enable
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.common.utils.stateInBackground
import io.paritytech.polkadotapp.feature_chats_api.domain.chatRequest.ChatRequestServiceCoordinator
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatBotStateController
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentWorkerStarter
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageServiceStarter
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthMixin
import io.paritytech.polkadotapp.feature_fund_api.domain.AutoConvertDepositService
import io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts.ExploreProductsService
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProductsWarmUp
import io.paritytech.polkadotapp.feature_settings_impl.domain.interactors.SyncPriceCurrencyChange
import io.paritytech.polkadotapp.feature_splash_api.presentation.SplashPassedObserver
import io.paritytech.polkadotapp.feature_sso_impl.domain.SsoService
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocator
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import io.paritytech.polkadotapp.tools_jwt_auth_impl.domain.warmUp.JwtAuthWarmUpService
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.plus
import timber.log.Timber
import javax.inject.Inject

@HiltViewModel
class RootViewModel @Inject constructor(
    private val rootInteractor: RootInteractor,
    private val syncPriceCurrencyChange: SyncPriceCurrencyChange,
    private val deeplinkHandler: DeepLinkHandler,
    private val splashPassedObserver: SplashPassedObserver,
    private val remoteConfigService: RemoteConfigService,
    private val depositService: AutoConvertDepositService,
    private val coinageServiceStarter: CoinageServiceStarter,
    private val externalPaymentWorkerStarter: ExternalPaymentWorkerStarter,
    private val statementStoreSlotAllocator: StatementStoreSlotAllocator,
    private val ssoService: SsoService,
    private val chatRequestServiceCoordinator: ChatRequestServiceCoordinator,
    private val exploreProductsService: ExploreProductsService,
    private val fundingProductsWarmUp: FundingProductsWarmUp,
    private val jwtAuthWarmUpService: JwtAuthWarmUpService,
    chatBotStateController: ChatBotStateController,
    chatEngine: ChatEngine,
    observeAccountOnboardingStatus: ObserveAccountOnboardingStatusUseCase,
    bottomNavHeightProvider: BottomNavHeightProvider,
    chainHealthMixinFactory: ChainHealthMixin.Factory,
    coroutineDispatchers: CoroutineDispatchers,
) : BaseViewModel(), RootContract {
    override val chatOverlays = chatEngine.observeActiveOverlays()
    override val isOnboarded = observeAccountOnboardingStatus().map { it.isOnboarded }
    override val bottomNavHeight = bottomNavHeightProvider.heightDp
    override val chainsHealth = chainHealthMixinFactory.create(this).model

    private val networkStatusTooltipDismissed = MutableStateFlow(!rootInteractor.shouldShowNetworkStatusTooltip())
    private val chainRingsOnScreen = chainsHealth.map { it.rows.isNotEmpty() }

    override val isNetworkStatusTooltipVisible = combine(
        networkStatusTooltipDismissed,
        isOnboarded,
        chainRingsOnScreen,
    ) { dismissed, onboarded, ringsOnScreen ->
        !dismissed && onboarded && ringsOnScreen
    }.stateInBackground(initialValue = false)

    private val servicesScope = ComputationalScope(viewModelScope + coroutineDispatchers.computation)

    init {
        with(servicesScope) {
            // Durability must not hang off the remote config fetch: a failed sync would otherwise skip
            // handoff release and transaction recovery for the whole session.
            coinageServiceStarter.start()
            externalPaymentWorkerStarter.start()

            launch {
                remoteConfigService.sync()
                    .onSuccess { launchServicesBasedOnRemoteConfig() }
            }

            launch { jwtAuthWarmUpService.warmUpToken() }
            launch { chatRequestServiceCoordinator.runChatRequestServices() }
            launch { chatBotStateController.applyDefaultBotStates() }
            launch { rootInteractor.printAccountAddresses() }
            launch { checkDevReset() }
        }
        watchSsoEvents(servicesScope)
    }

    private fun launchServicesBasedOnRemoteConfig() {
        with(servicesScope) {
            launch { rootInteractor.syncPrices() }
            launch { depositService.startObserveAndConvert() }
            launch { syncPriceCurrencyChange.startObserving() }
            launch { statementStoreSlotAllocator.scheduleSlotRenewals() }

            rootInteractor.startUpdateSystems().shareInBackground()
        }
        warmUpWebProducts(servicesScope)
    }

    private fun warmUpWebProducts(scope: ComputationalScope) {
        if (FeatureOption.BROWSE_TAB.isEnabled) {
            scope.launch { exploreProductsService.warmUpExploreLoading() }
        }
        scope.launch { fundingProductsWarmUp.warmUp() }
    }

    override val showDevResetPrompt = MutableStateFlow(false)

    override fun onDevResetStartOverClick() {
        launch { rootInteractor.clearAllAndClose() }
    }

    override fun onDevResetDismissClick() {
        showDevResetPrompt.disable()
    }

    override fun dismissNetworkStatusTooltip() {
        if (networkStatusTooltipDismissed.value) return
        networkStatusTooltipDismissed.value = true
        rootInteractor.markNetworkStatusTooltipShown()
    }

    private suspend fun checkDevReset() {
        if (rootInteractor.isDevResetNeeded()) {
            showDevResetPrompt.enable()
        }
    }

    private val _showDeeplinkOutcome = OneShotEventChannel<DeeplinkProcessingOutcome>()
    val showDeeplinkOutcome = _showDeeplinkOutcome.receiveAsFlow()

    fun handleDeepLink(uri: Uri) = launch {
        splashPassedObserver.awaitSplashPassed()
        val outcome = deeplinkHandler.handle(uri)
            .onFailure { Timber.e(it, "Failed to handle deeplink") }
            .flatten()
        _showDeeplinkOutcome.trySend(outcome)
    }

    private fun watchSsoEvents(scope: CoroutineScope) {
        if (FeatureOption.LINKED_DEVICES.isEnabled) {
            ssoService.watchSsoEvents().launchIn(scope)
        }
    }
}
