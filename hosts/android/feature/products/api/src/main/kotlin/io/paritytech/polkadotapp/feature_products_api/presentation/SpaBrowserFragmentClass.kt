package io.paritytech.polkadotapp.feature_products_api.presentation

import javax.inject.Qualifier

/** Qualifies the SPA browser fragment's class name, for overlays that stay off while a product is on screen. */
@Qualifier
@Retention(AnnotationRetention.BINARY)
annotation class SpaBrowserFragmentClass
