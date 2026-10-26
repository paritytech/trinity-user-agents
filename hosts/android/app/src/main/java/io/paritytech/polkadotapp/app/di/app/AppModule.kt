package io.paritytech.polkadotapp.app.di.app

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.app.root.RealActivityIntentProvider
import io.paritytech.polkadotapp.app.root.RealAppLifecycleObserver
import io.paritytech.polkadotapp.app.root.data.debug.RealIssueReportApi
import io.paritytech.polkadotapp.app.root.domain.debug.IssueReportApi
import io.paritytech.polkadotapp.app.root.network.RemoteConfigIdentityBackendUrlProvider
import io.paritytech.polkadotapp.app.telemetry.AndroidHostEventOriginProvider
import io.paritytech.polkadotapp.app.telemetry.AndroidHostLaunchIdProvider
import io.paritytech.polkadotapp.app.telemetry.AndroidHostMonotonicTimeProvider
import io.paritytech.polkadotapp.app.telemetry.AndroidHostProcessTelemetryInitializer
import io.paritytech.polkadotapp.app.telemetry.AndroidSentryEventCapture
import io.paritytech.polkadotapp.app.telemetry.FeatureOptionHostTelemetryEnabledProvider
import io.paritytech.polkadotapp.app.telemetry.HostLaunchIdProvider
import io.paritytech.polkadotapp.app.telemetry.HostTelemetryEnabledProvider
import io.paritytech.polkadotapp.app.telemetry.SentryEventCapture
import io.paritytech.polkadotapp.app.telemetry.SentryHostEventCapture
import io.paritytech.polkadotapp.common.data.network.IdentityBackendUrlProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.telemetry.HostMonotonicTimeProvider
import io.paritytech.polkadotapp.common.presentation.ActivityIntentProvider
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface AppModule {
    @Binds
    fun bindIssueReportApi(impl: RealIssueReportApi): IssueReportApi

    @Binds
    fun bindActivityIntentProvider(impl: RealActivityIntentProvider): ActivityIntentProvider

    @Binds
    @Singleton
    fun bindAppLifecycleObserver(impl: RealAppLifecycleObserver): AppLifecycleObserver

    @Binds
    @Singleton
    fun bindIdentityBackendUrlProvider(impl: RemoteConfigIdentityBackendUrlProvider): IdentityBackendUrlProvider

    @Binds
    @IntoSet
    fun bindHostProcessTelemetryInitializer(impl: AndroidHostProcessTelemetryInitializer): AppInitializer

    @Binds
    fun bindHostEventCapture(impl: SentryHostEventCapture): HostEventCapture

    @Binds
    fun bindSentryEventCapture(impl: AndroidSentryEventCapture): SentryEventCapture

    @Binds
    fun bindHostTelemetryEnabledProvider(
        impl: FeatureOptionHostTelemetryEnabledProvider,
    ): HostTelemetryEnabledProvider

    @Binds
    fun bindHostEventOriginProvider(impl: AndroidHostEventOriginProvider): HostEventOriginProvider

    @Binds
    fun bindHostMonotonicTimeProvider(impl: AndroidHostMonotonicTimeProvider): HostMonotonicTimeProvider

    @Binds
    @Singleton
    fun bindHostLaunchIdProvider(impl: AndroidHostLaunchIdProvider): HostLaunchIdProvider
}
