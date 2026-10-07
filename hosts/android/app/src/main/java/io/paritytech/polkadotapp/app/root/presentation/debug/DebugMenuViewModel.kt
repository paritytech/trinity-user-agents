package io.paritytech.polkadotapp.app.root.presentation.debug

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.app.root.domain.debug.ClearBackupUseCase
import io.paritytech.polkadotapp.app.root.domain.debug.CollectLogsUseCase
import io.paritytech.polkadotapp.app.root.domain.debug.GetAddressUseCase
import io.paritytech.polkadotapp.app.root.domain.debug.GetWalletMnemonicUseCase
import io.paritytech.polkadotapp.app.root.domain.debug.RandomizeAccountUseCase
import io.paritytech.polkadotapp.app.root.domain.debug.RestartAppUseCase
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.feature_coinage_api.domain.debug.CoinageDebugSettings
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store.JWTTokenStore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject

@HiltViewModel
class DebugMenuViewModel @Inject constructor(
    private val clipboardService: ClipboardService,
    private val clearBackupUseCase: ClearBackupUseCase,
    private val collectLogsUseCase: CollectLogsUseCase,
    private val getAddressUseCase: GetAddressUseCase,
    private val getWalletMnemonicUseCase: GetWalletMnemonicUseCase,
    private val router: RootRouter,
    private val randomizeAccountUseCase: RandomizeAccountUseCase,
    private val dotNsResolver: DotNsResolver,
    private val jwtTokenStore: JWTTokenStore,
    private val coinageDebugSettings: CoinageDebugSettings,
    private val productRuntimeSettings: ProductRuntimeSettings,
    private val restartAppUseCase: RestartAppUseCase,
) : BaseViewModel(), DebugMenuContract {
    override val state = MutableStateFlow(DebugMenuState())

    init {
        refreshJWTTokenState()
        refreshCoinageDebugWidgetsState()
        refreshTruapiRuntimeState()
    }

    override fun onBackClick() {
        router.back()
    }

    override fun onClearBackupClick() = launchUnit {
        state.update { it.copy(isClearingBackup = true) }

        clearBackupUseCase()
            .onFailure(::showError)

        state.update { it.copy(isClearingBackup = false) }
    }

    override fun onShareLogsClick() = launchUnit {
        state.update { it.copy(isSharingLogs = true) }

        collectLogsUseCase()
            .onFailure(::showError)

        state.update { it.copy(isSharingLogs = false) }
    }

    override fun onCopyWalletAccountClick() = launchUnit {
        clipboardService.setPrimaryClip(getAddressUseCase.wallet())
    }

    override fun onCopyWalletMnemonicClick() = launchUnit {
        clipboardService.setPrimaryClip(getWalletMnemonicUseCase().words)
    }

    override fun onProductBotsClick() {
        router.openProductBotsManagement()
    }

    override fun onPocketFacePreviewClick() {
        router.openPocketFacePreview()
    }

    override fun onRandomizeAccountClick() = launchUnit {
        randomizeAccountUseCase()
    }

    override fun onOpenSpaBrowserClick() {
        state.update { it.copy(showSpaBrowserDialog = true) }
    }

    override fun onSpaBrowserUrlEntered(url: String) {
        state.update { it.copy(showSpaBrowserDialog = false) }
        router.openSpaBrowser(SpaBrowserPayload.ByUrl(Urls.ensureHttpsProtocol(url)))
    }

    override fun onSpaBrowserDialogDismissed() {
        state.update { it.copy(showSpaBrowserDialog = false) }
    }

    override fun onClearDotNsCacheClick() = launchUnit {
        dotNsResolver.clearCache()
        showMessage("DotNs cache cleared")
    }

    override fun onClearJWTTokenClick() {
        jwtTokenStore.deleteToken()
        refreshJWTTokenState()
    }

    override fun onCoinageDebugWidgetsToggled(enabled: Boolean) = launchUnit {
        coinageDebugSettings.setWidgetsEnabled(enabled)
        state.update { it.copy(coinageDebugWidgetsEnabled = enabled) }
    }

    /**
     * Each runtime keeps its own storage namespace and its own permission
     * memory, so a product switched mid-session reads an empty store and the
     * host it is already talking to belongs to the other runtime. Restarting
     * makes the switch a clean boot instead.
     */
    override fun onTruapiRuntimeToggled(enabled: Boolean) {
        val previous = state.value.truapiRuntimeEnabled
        productRuntimeSettings.setTrUAPIRuntimeEnabled(enabled)
        state.update { it.copy(runtimeRestartRevertsTo = previous) }
        refreshTruapiRuntimeState()
    }

    override fun onRuntimeRestartConfirmed() {
        restartAppUseCase()
    }

    override fun onRuntimeRestartCancelled() {
        state.value.runtimeRestartRevertsTo?.let(productRuntimeSettings::setTrUAPIRuntimeEnabled)
        state.update { it.copy(runtimeRestartRevertsTo = null) }
        refreshTruapiRuntimeState()
    }

    private fun refreshJWTTokenState() {
        state.update { it.copy(hasJWTToken = jwtTokenStore.fetchToken() != null) }
    }

    private fun refreshCoinageDebugWidgetsState() = launchUnit {
        val enabled = coinageDebugSettings.areWidgetsEnabled()
        state.update { it.copy(coinageDebugWidgetsEnabled = enabled) }
    }

    private fun refreshTruapiRuntimeState() {
        state.update { it.copy(truapiRuntimeEnabled = productRuntimeSettings.isTrUAPIRuntimeEnabled()) }
    }
}
