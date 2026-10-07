package io.paritytech.polkadotapp.feature_products_impl.di

import android.content.Context
import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ExternalExtensionProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.search.ChatSearchResultProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.domain.ProductAccountIdProvider
import io.paritytech.polkadotapp.feature_products_api.domain.ProductRequestAccountResolver
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.AccountsProtocol
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.MembersRingLocator
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.deriveEntropy.DeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCollection
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFaceSource
import io.paritytech.polkadotapp.feature_products_api.domain.product.ProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.PreimageSubmitSponsoring
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.StatementStoreSubmissionSponsoring
import io.paritytech.polkadotapp.feature_products_api.domain.sponsoring.TransactionSponsoring
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserFragmentClass
import io.paritytech.polkadotapp.feature_products_api.presentation.deeplink.ProductDeepLinkGate
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_impl.data.config.RemoteConfigFundingDomainProvider
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.PocketCardRepository
import io.paritytech.polkadotapp.feature_products_impl.data.pocket.RealPocketCardRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.BrowserTabRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductFundingOperationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealBrowserTabRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealProductFundingOperationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealProductIntegrationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealProductRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealRingVrfKeyRegistrationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RealTopUpRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.RingVrfKeyRegistrationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.repository.TopUpRepository
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.RealScheduledProductNotificationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import io.paritytech.polkadotapp.feature_products_impl.data.storage.AssetContainerScriptProvider
import io.paritytech.polkadotapp.feature_products_impl.data.storage.ContainerScriptProvider
import io.paritytech.polkadotapp.feature_products_impl.data.storage.ProductLocalStorage
import io.paritytech.polkadotapp.feature_products_impl.data.storage.RealProductLocalStorage
import io.paritytech.polkadotapp.feature_products_impl.data.storage.RealTopUpSourceStorage
import io.paritytech.polkadotapp.feature_products_impl.data.storage.TopUpSourceStorage
import io.paritytech.polkadotapp.feature_products_impl.domain.ProductAccountDerivationUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.RealProductRequestAccountResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.RealAccountsProtocol
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.RealRingVrfKeySource
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.RingVrfKeySource
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator.RealMembersRingLocator
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RealRingVrfKeyRegistry
import io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.registry.RingVrfKeyRegistry
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.external.ProductExternalExtensionProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu.ProductChatMenuInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.menu.RealProductChatMenuInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.browser.RealProductSessionController
import io.paritytech.polkadotapp.feature_products_impl.domain.deriveEntropy.RealDeriveEntropyUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts.ExploreProductsService
import io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts.RealExploreProductsService
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProductsWarmUp
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingProductsWarmUp
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.AllowanceKeyStorage
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.allowance.RealAllowanceKeyStorage
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring.RealStatementStoreSubmissionSponsoring
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring.SponsorPreimageWithBulletin
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.sponsoring.SponsorReviveCallsWithPgas
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.RealProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.ProductOperationService
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.RealProductOperationService
import io.paritytech.polkadotapp.feature_products_impl.domain.origin.ProductAccountOrigins
import io.paritytech.polkadotapp.feature_products_impl.domain.origin.RealProductAccountOrigins
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RealRequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.RequestPaymentUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.AutoAllowProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.RealWhitelistedProductsProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.WhitelistedProductsProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.AccountAccessPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.BalanceAccessPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.DeviceCapabilityPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.ProductPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.RemotePermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.UserIdentityAccessPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.AssetPinnedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.OkHttpRemoteFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PinnedPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PrefsDebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RealPocketCollection
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RealPocketFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RealPocketImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.RemoteFaceSource
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.product.RealProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_impl.domain.product.RealProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.product.RealProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement.ProductBotManagementInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.productBotManagement.RealProductBotManagementInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.runtime.PrefsProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.search.ProductChatSearchResultProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.serialization.JsWidgetSerializer
import io.paritytech.polkadotapp.feature_products_impl.domain.serialization.ScaleWidgetSerializer
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.RealSpaBrowserInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.SpaBrowserInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.ExecuteTopUpUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.RealExecuteTopUpUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.RealTopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.TrUAPIPocketFaceStreams
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.RealResolveProductUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductServingHostResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RealProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.RealWorkerBootFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.WorkerBootFactory
import io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink.PocketDeepLinkHandler
import io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink.PocketScanContentParser
import io.paritytech.polkadotapp.feature_products_impl.presentation.initialization.ProductWorkerInitializer
import io.paritytech.polkadotapp.feature_products_impl.presentation.initialization.TopUpResumeInitializer
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.SpaBrowserFragment
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.RuntimeSelectingSpaHost
import io.paritytech.polkadotapp.feature_scan_api.domain.ScanContentParser
import okhttp3.Call
import okhttp3.OkHttpClient
import java.util.concurrent.TimeUnit
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface ProductsModule {
    @Binds
    @Singleton
    fun bindWidgetSerializer(impl: ScaleWidgetSerializer): JsWidgetSerializer

    @Binds
    @Singleton
    fun bindRemoteFaceSource(impl: OkHttpRemoteFaceSource): RemoteFaceSource

    @Binds
    @Singleton
    fun bindProductSessionController(impl: RealProductSessionController): ProductSessionController

    @Binds
    @Singleton
    fun bindBrowserTabRepository(impl: RealBrowserTabRepository): BrowserTabRepository

    @Binds
    @Singleton
    fun bindSpaHost(impl: RuntimeSelectingSpaHost): SpaHost

    @Binds
    @Singleton
    fun bindProductAccountIdProvider(impl: ProductAccountDerivationUseCase): ProductAccountIdProvider

    @Binds
    @Singleton
    fun bindProductRepository(impl: RealProductRepository): ProductRepository

    @Binds
    @Singleton
    fun bindResolveProductUseCase(impl: RealResolveProductUseCase): ResolveProductUseCase

    @Binds
    @Singleton
    fun bindPocketCardRepository(impl: RealPocketCardRepository): PocketCardRepository

    @Binds
    @Singleton
    fun bindPinnedPocketCards(impl: AssetPinnedPocketCards): PinnedPocketCards

    @Binds
    @Singleton
    fun bindPocketCardStore(impl: RealPocketCollection): PocketCardStore

    @Binds
    fun bindPocketCollection(impl: PocketCardStore): PocketCollection

    @Binds
    fun bindPocketFaceSource(impl: RealPocketFaceSource): PocketFaceSource

    @Binds
    fun bindPocketFaceStreams(impl: TrUAPIPocketFaceStreams): PocketFaceStreams

    @Binds
    fun bindPocketImageResolver(impl: RealPocketImageResolver): PocketImageResolver

    @Binds
    @Singleton
    fun bindServingHostResolver(impl: ProductServingHostResolver): DotNsServingHostResolver

    @Binds
    @Singleton
    fun bindProductContentWarmUp(impl: RealProductContentWarmUp): ProductContentWarmUp

    @Binds
    @Singleton
    fun bindContainerScriptProvider(impl: AssetContainerScriptProvider): ContainerScriptProvider

    @Binds
    @Singleton
    fun bindProductBotManagementInteractor(impl: RealProductBotManagementInteractor): ProductBotManagementInteractor

    @Binds
    @IntoSet
    fun bindProductExternalExtensionProvider(impl: ProductExternalExtensionProvider): ExternalExtensionProvider

    @Binds
    @IntoSet
    fun bindPocketDeepLinkHandler(impl: PocketDeepLinkHandler): DeepLinkHandler

    @Binds
    fun bindProductLocalStorage(impl: RealProductLocalStorage): ProductLocalStorage

    @Binds
    fun bindTopUpSourceStorage(impl: RealTopUpSourceStorage): TopUpSourceStorage

    @Binds
    fun bindTopUpRepository(impl: RealTopUpRepository): TopUpRepository

    @Binds
    @Singleton
    fun bindProductWorkerRefCounter(impl: RealProductWorkerRefCounter): ProductWorkerRefCounter

    @Binds
    fun bindWorkerBootFactory(impl: RealWorkerBootFactory): WorkerBootFactory

    @Binds
    @Singleton
    fun bindProductOperationService(impl: RealProductOperationService): ProductOperationService

    @Binds
    fun bindProductFundingOperationRepository(impl: RealProductFundingOperationRepository): ProductFundingOperationRepository

    @Binds
    @IntoSet
    fun bindProductWorkerInitializer(impl: ProductWorkerInitializer): AppInitializer

    @Binds
    @IntoSet
    fun bindTopUpResumeInitializer(impl: TopUpResumeInitializer): AppInitializer

    @Binds
    fun bindProductAccountOrigins(impl: RealProductAccountOrigins): ProductAccountOrigins

    @Binds
    fun bindMembersRingLocator(impl: RealMembersRingLocator): MembersRingLocator

    @Binds
    fun bindRingVrfKeyRegistrationRepository(
        impl: RealRingVrfKeyRegistrationRepository
    ): RingVrfKeyRegistrationRepository

    @Binds
    fun bindRingVrfKeyRegistry(impl: RealRingVrfKeyRegistry): RingVrfKeyRegistry

    @Binds
    fun bindRingVrfKeySource(impl: RealRingVrfKeySource): RingVrfKeySource

    @Binds
    @Singleton
    fun bindProductPermissionRepository(impl: RealProductPermissionRepository): ProductPermissionRepository

    @Binds
    fun bindProductPermissionGuard(impl: RealProductPermissionGuard): ProductPermissionGuard

    @Binds
    fun bindRemotePermissionHandler(
        impl: RemotePermissionHandler,
    ): ProductPermissionHandler<ProductPermission.RemotePermission>

    @Binds
    fun bindAccountAccessPermissionHandler(
        impl: AccountAccessPermissionHandler,
    ): ProductPermissionHandler<ProductPermission.AccountAccess>

    @Binds
    fun bindBalanceAccessPermissionHandler(
        impl: BalanceAccessPermissionHandler,
    ): ProductPermissionHandler<ProductPermission.BalanceAccess>

    @Binds
    fun bindDeviceCapabilityPermissionHandler(
        impl: DeviceCapabilityPermissionHandler,
    ): ProductPermissionHandler<ProductPermission.DeviceCapability>

    @Binds
    fun bindUserIdentityAccessPermissionHandler(
        impl: UserIdentityAccessPermissionHandler,
    ): ProductPermissionHandler<ProductPermission.UserIdentityAccess>

    @Binds
    @Singleton
    fun bindProductScriptResolver(impl: RealProductScriptResolver): ProductScriptResolver

    @Binds
    @Singleton
    fun bindProductRegistrar(impl: RealProductRegistrar): ProductRegistrar

    @Binds
    @Singleton
    fun bindProductIntegrationRepository(impl: RealProductIntegrationRepository): ProductIntegrationRepository

    @Binds
    fun bindSpaBrowserInteractor(impl: RealSpaBrowserInteractor): SpaBrowserInteractor

    @Binds
    fun bindProductChatMenuInteractor(impl: RealProductChatMenuInteractor): ProductChatMenuInteractor

    @Binds
    @Singleton
    fun bindAllowanceKeyStorage(impl: RealAllowanceKeyStorage): AllowanceKeyStorage

    @Binds
    @Singleton
    fun bindAccountsProtocol(impl: RealAccountsProtocol): AccountsProtocol

    @Binds
    fun bindTransactionSponsoring(impl: SponsorReviveCallsWithPgas): TransactionSponsoring

    @Binds
    fun bindPreimageSubmitSponsoring(impl: SponsorPreimageWithBulletin): PreimageSubmitSponsoring

    @Binds
    fun bindStatementStoreSubmissionSponsoring(impl: RealStatementStoreSubmissionSponsoring): StatementStoreSubmissionSponsoring

    @Binds
    fun bindExploreProductsService(impl: RealExploreProductsService): ExploreProductsService

    @Binds
    fun bindFundingProductsWarmUp(impl: RealFundingProductsWarmUp): FundingProductsWarmUp

    @Binds
    @Singleton
    fun bindScheduledProductNotificationRepository(
        impl: RealScheduledProductNotificationRepository,
    ): ScheduledProductNotificationRepository

    @Binds
    @Singleton
    fun bindProductNotificationScheduler(impl: RealProductNotificationScheduler): ProductNotificationScheduler

    @Binds
    fun bindProductRequestAccountResolver(impl: RealProductRequestAccountResolver): ProductRequestAccountResolver

    @Binds
    @Singleton
    fun bindFundingDomainProvider(impl: RemoteConfigFundingDomainProvider): FundingDomainProvider

    @Binds
    fun bindWhitelistedProductsProvider(impl: RealWhitelistedProductsProvider): WhitelistedProductsProvider

    @Binds
    fun bindDeriveEntropyUseCase(impl: RealDeriveEntropyUseCase): DeriveEntropyUseCase

    @Binds
    fun bindExecuteTopUpUseCase(impl: RealExecuteTopUpUseCase): ExecuteTopUpUseCase

    @Binds
    @Singleton
    fun bindTopUpService(impl: RealTopUpService): TopUpService

    @Binds
    fun bindRequestPaymentUseCase(impl: RealRequestPaymentUseCase): RequestPaymentUseCase

    companion object {
        @Provides
        @Singleton
        @TrUAPIChainHttpClient
        fun provideTrUAPIChainHttpClient(shared: OkHttpClient): OkHttpClient =
            shared.newBuilder()
                // Chain sockets are long-lived subscriptions: the shared client's
                // read timeout would kill them when idle, so detect dead peers
                // with pings instead. newBuilder keeps the shared pool/dispatcher.
                .readTimeout(0, TimeUnit.SECONDS)
                .pingInterval(CHAIN_SOCKET_PING_SECONDS, TimeUnit.SECONDS)
                .build()

        private const val CHAIN_SOCKET_PING_SECONDS = 30L

        @Provides
        @SpaBrowserFragmentClass
        fun provideSpaBrowserFragmentClass(): String = SpaBrowserFragment::class.java.name

        /** `OkHttpClient` is a `Call.Factory`; naming the interface keeps the face source testable. */
        @Provides
        @Singleton
        fun provideFaceCallFactory(client: OkHttpClient): Call.Factory = client

        @Provides
        @Singleton
        fun provideDebugPocketCards(@ApplicationContext context: Context): DebugPocketCards =
            PrefsDebugPocketCards(
                prefs = context.getSharedPreferences("debug_pocket_cards", Context.MODE_PRIVATE),
                isDebugBuild = BuildConfig.DEBUG,
            )

        @Provides
        @Singleton
        fun provideProductRuntimeSettings(@ApplicationContext context: Context): ProductRuntimeSettings =
            PrefsProductRuntimeSettings(
                prefs = context.getSharedPreferences("product_runtime_settings", Context.MODE_PRIVATE),
                isDebugBuild = BuildConfig.DEBUG,
            )

        @Provides
        @Singleton
        fun providePermissionRequester(
            real: RealProductPermissionRequester,
            whitelistedProductsProvider: WhitelistedProductsProvider,
        ): ProductPermissionRequester {
            return AutoAllowProductPermissionRequester(whitelistedProductsProvider, real)
        }

        @Provides
        @IntoSet
        fun providePocketScanContentParser(handler: PocketDeepLinkHandler): ScanContentParser =
            PocketScanContentParser(handler)

        @Provides
        @Singleton
        fun provideProductDeepLinkGate(fundingDomainProvider: FundingDomainProvider): ProductDeepLinkGate =
            ProductDeepLinkGate(
                arbitraryProductsEnabled = FeatureOption.ARBITRARY_PRODUCTS.isEnabled,
                fundingDomainProvider = fundingDomainProvider,
            )

        @Provides
        @IntoSet
        fun provideProductChatSearchResultProvider(
            productRepository: ProductRepository,
            productsRouter: ProductsRouter,
        ): ChatSearchResultProvider {
            return ProductChatSearchResultProvider(
                arbitraryProductsEnabled = FeatureOption.ARBITRARY_PRODUCTS.isEnabled,
                productRepository = productRepository,
                productsRouter = productsRouter,
            )
        }
    }
}
