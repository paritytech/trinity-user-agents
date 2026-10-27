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
import io.paritytech.polkadotapp.app.telemetry.HostProcessTelemetryInitializer
import io.paritytech.polkadotapp.app.telemetry.HostTelemetryEnabledProvider
import io.paritytech.polkadotapp.app.telemetry.RealHostEventCapture
import io.paritytech.polkadotapp.app.telemetry.RealHostEventOriginProvider
import io.paritytech.polkadotapp.app.telemetry.RealHostMonotonicTimeProvider
import io.paritytech.polkadotapp.app.telemetry.RealHostTelemetryEnabledProvider
import io.paritytech.polkadotapp.app.telemetry.RealSentryEventCapture
import io.paritytech.polkadotapp.app.telemetry.SentryEventCapture
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
    fun bindHostProcessTelemetryInitializer(impl: HostProcessTelemetryInitializer): AppInitializer

    @Binds
    fun bindHostEventCapture(impl: RealHostEventCapture): HostEventCapture

    @Binds
    fun bindSentryEventCapture(impl: RealSentryEventCapture): SentryEventCapture

    @Binds
    fun bindHostTelemetryEnabledProvider(
        impl: RealHostTelemetryEnabledProvider,
    ): HostTelemetryEnabledProvider

    @Binds
    fun bindHostEventOriginProvider(impl: RealHostEventOriginProvider): HostEventOriginProvider

    @Binds
    fun bindHostMonotonicTimeProvider(impl: RealHostMonotonicTimeProvider): HostMonotonicTimeProvider
}
