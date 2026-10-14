package io.paritytech.polkadotapp.feature_sso_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_scan_api.domain.DeeplinkScanContentParser
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import io.paritytech.polkadotapp.feature_sso_api.di.SsoPairing
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.OwnDevicesJournal
import io.paritytech.polkadotapp.feature_sso_api.domain.SsoHandshakeUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.RegisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.SyncDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.devices.UnregisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_impl.data.RealOwnDevicesJournal
import io.paritytech.polkadotapp.feature_sso_impl.data.RealSsoHandshakeProtocol
import io.paritytech.polkadotapp.feature_sso_impl.data.RealSsoHandshakeRepository
import io.paritytech.polkadotapp.feature_sso_impl.data.SsoHandshakeProtocol
import io.paritytech.polkadotapp.feature_sso_impl.data.SsoHandshakeRepository
import io.paritytech.polkadotapp.feature_sso_impl.deeplink.PairDeepLinkHandler
import io.paritytech.polkadotapp.feature_sso_impl.domain.RealGetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_impl.domain.RealSsoHandshakeUseCase
import io.paritytech.polkadotapp.feature_sso_impl.domain.devices.RealRegisterDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_impl.domain.devices.RealSyncDeviceUseCase
import io.paritytech.polkadotapp.feature_sso_impl.domain.devices.RealUnregisterDeviceUseCase

@Module
@InstallIn(SingletonComponent::class)
internal interface SsoModule {
    @Binds
    fun bindSsoHandshakeRepository(impl: RealSsoHandshakeRepository): SsoHandshakeRepository

    @Binds
    fun bindSsoHandshakeProtocol(impl: RealSsoHandshakeProtocol): SsoHandshakeProtocol

    @Binds
    fun bindSsoHandshakeUseCase(impl: RealSsoHandshakeUseCase): SsoHandshakeUseCase

    @Binds
    fun bindGetActiveSsoSessionsUseCase(impl: RealGetActiveSsoSessionsUseCase): GetActiveSsoSessionsUseCase

    @Binds
    fun bindRegisterDeviceUseCase(impl: RealRegisterDeviceUseCase): RegisterDeviceUseCase

    @Binds
    fun bindSyncDeviceUseCase(impl: RealSyncDeviceUseCase): SyncDeviceUseCase

    @Binds
    fun bindUnregisterDeviceUseCase(impl: RealUnregisterDeviceUseCase): UnregisterDeviceUseCase

    @Binds
    fun bindOwnDevicesJournal(impl: RealOwnDevicesJournal): OwnDevicesJournal

    @Binds
    @IntoSet
    fun bindPairDeepLinkHandlerIntoSet(impl: PairDeepLinkHandler): DeepLinkHandler

    @Binds
    @SsoPairing
    fun bindPairDeepLinkHandler(impl: PairDeepLinkHandler): DeepLinkHandler

    companion object {
        @Provides
        @IntoSet
        fun provideSsoPairingScanContentParser(
            handler: PairDeepLinkHandler,
        ): ScanContentParser = DeeplinkScanContentParser(handler)
    }
}
