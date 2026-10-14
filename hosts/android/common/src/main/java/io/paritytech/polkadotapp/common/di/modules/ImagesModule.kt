package io.paritytech.polkadotapp.common.di.modules

import android.content.Context
import coil.ImageLoader
import coil.decode.SvgDecoder
import coil.decode.VideoFrameDecoder
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.Multibinds
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.common.data.image.fetchers.JsImageFetcher
import io.paritytech.polkadotapp.common.data.image.interceptors.ImageLoaderInterceptor
import io.paritytech.polkadotapp.common.data.image.keyers.JsImageKeyer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
class ImagesModule {
    @Provides
    @Singleton
    fun provideJsImageFetcherFactory(
        @ApplicationContext context: Context,
        dispatchers: CoroutineDispatchers,
    ) = JsImageFetcher.Factory(context, dispatchers)

    @Provides
    @Singleton
    fun imageLoader(
        @ApplicationContext context: Context,
        jsImageFetcherFactory: JsImageFetcher.Factory,
        componentRegistrars: Set<@JvmSuppressWildcards ImageLoaderComponentRegistrar>,
    ) =
        ImageLoader.Builder(context)
            .components {
                add(ImageLoaderInterceptor())

                add(JsImageKeyer())

                add(jsImageFetcherFactory)

                componentRegistrars.forEach { it.register(this) }

                add(SvgDecoder.Factory())
                add(VideoFrameDecoder.Factory())
            }
            .build()
}

@Module
@InstallIn(SingletonComponent::class)
interface ImageLoaderComponentRegistrarModule {
    @Multibinds
    fun imageLoaderComponentRegistrars(): Set<ImageLoaderComponentRegistrar>
}
