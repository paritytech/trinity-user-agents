package io.paritytech.polkadotapp.common.di.modules

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.data.keypair.RealClientKeypairStore
import io.paritytech.polkadotapp.common.data.memory.ComputationalCache
import io.paritytech.polkadotapp.common.data.memory.RealComputationalCache
import io.paritytech.polkadotapp.common.data.platform.BatteryService
import io.paritytech.polkadotapp.common.data.platform.RealBatteryService
import io.paritytech.polkadotapp.common.data.storage.SingleValueStorageFactory
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.data.storage.file.FileCache
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.data.storage.file.InternalFileSystemCache
import io.paritytech.polkadotapp.common.data.storage.file.RealContentResolver
import io.paritytech.polkadotapp.common.data.storage.file.RealFileProvider
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.RealPreferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.RealEncryptedPreferences
import io.paritytech.polkadotapp.common.data.time.RealTimeProvider
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.CurrentTimeContext
import io.paritytech.polkadotapp.common.presentation.BrowserNavigator
import io.paritytech.polkadotapp.common.presentation.RealBrowserNavigator
import io.paritytech.polkadotapp.common.presentation.camera.CameraQrReader
import io.paritytech.polkadotapp.common.presentation.camera.RealCameraQrReader
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.clipboard.RealClipboardService
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.RootDeeplinkHandler
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatters
import io.paritytech.polkadotapp.common.presentation.formatters.number.RealNumberFormatterFactory
import io.paritytech.polkadotapp.common.presentation.formatters.number.createSharedFormatters
import io.paritytech.polkadotapp.common.presentation.formatters.space.InformationSizeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.space.RealInformationSizeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.RealTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.navigation.OverlayCoordinator
import io.paritytech.polkadotapp.common.presentation.navigation.RealOverlayCoordinator
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.RealAppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.presentation.resources.RealContextManager
import io.paritytech.polkadotapp.common.presentation.sharing.RealSharingManager
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.common.presentation.theme.RealAppThemeSelector
import io.paritytech.polkadotapp.common.presentation.ui.mixin.paste.PasteMixin
import io.paritytech.polkadotapp.common.presentation.ui.mixin.paste.PasteMixinFactory
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEventsMixin
import io.paritytech.polkadotapp.common.utils.calendar.RealCalendarEventsMixin
import io.paritytech.polkadotapp.common.utils.network.NetworkStateService
import io.paritytech.polkadotapp.common.utils.network.RealNetworkStateService
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.RealPermissionAsker
import io.paritytech.polkadotapp.design.theme.AppThemeSelector
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface CommonModule {
    @Binds
    fun bindCoroutineDispatchers(impl: RealCoroutineDispatchers): CoroutineDispatchers

    @Binds
    fun bindsComputationalCache(impl: RealComputationalCache): ComputationalCache

    @Binds
    fun bindTimeProvider(impl: RealTimeProvider): TimeProvider

    @Binds
    fun bindContextManager(impl: RealContextManager): ContextManager

    @Binds
    fun bindTimeFormatter(impl: RealTimeFormatter): TimeFormatter

    @Binds
    fun bindNumberFormatterFactory(impl: RealNumberFormatterFactory): NumberFormatterFactory

    @Binds
    fun bindSharingManager(impl: RealSharingManager): SharingManager

    @Binds
    fun bindPreferences(impl: RealPreferences): Preferences

    @Binds
    fun bindEncryptedPreferences(impl: RealEncryptedPreferences): EncryptedPreferences

    @Binds
    fun bindFileProvider(impl: RealFileProvider): FileProvider

    @Binds
    fun bindContentResolver(impl: RealContentResolver): ContentResolver

    @Binds
    fun bindClipboardService(impl: RealClipboardService): ClipboardService

    @Binds
    fun bindBatteryService(impl: RealBatteryService): BatteryService

    @Binds
    fun bindInformationSizeFormatter(impl: RealInformationSizeFormatter): InformationSizeFormatter

    @Binds
    fun bindOverlayCoordinator(impl: RealOverlayCoordinator): OverlayCoordinator

    @Binds
    @Singleton
    fun bindAppNotifier(impl: RealAppNotifier): AppNotifier

    @Binds
    fun bindPermissionAsker(impl: RealPermissionAsker): PermissionAsker

    @Binds
    @Singleton
    fun bindBrowserNavigator(impl: RealBrowserNavigator): BrowserNavigator

    @Binds
    @Singleton
    fun bindFileCache(impl: InternalFileSystemCache): FileCache

    @Binds
    @Singleton
    fun providePasteMixinFactory(impl: PasteMixinFactory): PasteMixin.Factory

    @Binds
    @Singleton
    fun findNetworkStateService(impl: RealNetworkStateService): NetworkStateService

    @Binds
    fun bindCameraQrReader(impl: RealCameraQrReader): CameraQrReader

    @Binds
    fun bindsCalendarEventsMixin(impl: RealCalendarEventsMixin): CalendarEventsMixin

    @Binds
    @Singleton
    fun bindAppThemeSelector(impl: RealAppThemeSelector): AppThemeSelector

    @Binds
    fun bindDeepLinkHandler(impl: RootDeeplinkHandler): DeepLinkHandler

    @Binds
    @Singleton
    fun bindClientKeypairStore(impl: RealClientKeypairStore): ClientKeypairStore

    companion object {
        @Provides
        @Singleton
        fun provideSharedNumberFormatters(numberFormatterFactory: NumberFormatterFactory): NumberFormatters {
            return numberFormatterFactory.createSharedFormatters()
        }

        @Provides
        @Singleton
        fun provideSingleValueStorageFactory(preferences: Preferences) =
            SingleValueStorageFactory(preferences)

        @Provides
        @OptIn(kotlin.time.ExperimentalTime::class)
        fun provideCurrentTimeContext(): CurrentTimeContext =
            CurrentTimeContext { kotlin.time.Clock.System.now() }
    }
}
