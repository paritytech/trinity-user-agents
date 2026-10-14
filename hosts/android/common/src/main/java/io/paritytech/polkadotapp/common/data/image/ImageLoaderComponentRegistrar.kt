package io.paritytech.polkadotapp.common.data.image

import coil.ComponentRegistry

/**
 * Lets a feature/tool module contribute its own Coil components (fetchers, keyers, decoders)
 * to the app-wide [coil.ImageLoader] built in [io.paritytech.polkadotapp.common.di.modules.ImagesModule]
 * without `common` depending on that module. Bind implementations with `@Binds @IntoSet`.
 */
interface ImageLoaderComponentRegistrar {
    fun register(builder: ComponentRegistry.Builder)
}
