package io.paritytech.polkadotapp.app.root.navigation

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.app.root.navigation.backup.BackupNavigator
import io.paritytech.polkadotapp.app.root.navigation.chats.ChatsNavigator
import io.paritytech.polkadotapp.app.root.navigation.fund.FundNavigator
import io.paritytech.polkadotapp.app.root.navigation.products.ProductsNavigator
import io.paritytech.polkadotapp.app.root.navigation.root.RootNavigator
import io.paritytech.polkadotapp.app.root.navigation.scan.ScanNavigator
import io.paritytech.polkadotapp.app.root.navigation.settings.SettingsNavigator
import io.paritytech.polkadotapp.app.root.navigation.splash.SplashNavigator
import io.paritytech.polkadotapp.app.root.navigation.sso.SsoNavigator
import io.paritytech.polkadotapp.app.root.navigation.username.UsernameNavigator
import io.paritytech.polkadotapp.app.root.navigation.videogame.VideoGameNavigator
import io.paritytech.polkadotapp.app.root.navigation.w3spay.W3sPayNavigator
import io.paritytech.polkadotapp.app.root.navigation.wallet.PocketNavigator
import io.paritytech.polkadotapp.app.root.presentation.root.RootRouter
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_backup_impl.BackupRouter
import io.paritytech.polkadotapp.feature_chats_impl.ChatsRouter
import io.paritytech.polkadotapp.feature_fund_impl.FundRouter
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_scan_impl.ScanRouter
import io.paritytech.polkadotapp.feature_settings_impl.SettingsRouter
import io.paritytech.polkadotapp.feature_splash_impl.presentation.SplashRouter
import io.paritytech.polkadotapp.feature_sso_impl.SsoRouter
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_w3spay_impl.W3sPayRouter
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface NavigatorsModule {
    companion object {
        @Provides
        @Singleton
        fun provideNavigatorHolder(contextManager: ContextManager): NavigationHolder =
            NavigationHolder(contextManager)
    }

    @Binds
    @Singleton
    fun bindRootRouter(impl: RootNavigator): RootRouter

    @Binds
    @Singleton
    fun bindOnboardingRouter(impl: BackupNavigator): BackupRouter

    @Binds
    @Singleton
    fun bindUsernamesRouter(impl: UsernameNavigator): UsernamesRouter

    @Binds
    @Singleton
    fun provideChatsRouter(impl: ChatsNavigator): ChatsRouter

    @Binds
    @Singleton
    fun providePocketRouter(impl: PocketNavigator): PocketRouter

    @Binds
    @Singleton
    fun provideFundRouter(impl: FundNavigator): FundRouter

    @Binds
    @Singleton
    fun bindSettingsRouter(impl: SettingsNavigator): SettingsRouter

    @Binds
    @Singleton
    fun bindSsoRouter(impl: SsoNavigator): SsoRouter

    @Binds
    @Singleton
    fun bindScanRouter(impl: ScanNavigator): ScanRouter

    @Binds
    @Singleton
    fun bindVideoGameRouter(impl: VideoGameNavigator): VideoGameRouter

    @Binds
    @Singleton
    fun bindSplashRouter(impl: SplashNavigator): SplashRouter

    @Binds
    @Singleton
    fun bindProductsRouter(impl: ProductsNavigator): ProductsRouter

    @Binds
    @Singleton
    fun bindSigningRouter(impl: ProductsNavigator): SigningRouter

    @Binds
    @Singleton
    fun bindW3sPayRouter(impl: W3sPayNavigator): W3sPayRouter
}
