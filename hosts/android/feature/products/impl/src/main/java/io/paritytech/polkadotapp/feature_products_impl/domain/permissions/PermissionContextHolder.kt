package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class PermissionContextHolder @Inject constructor() {
    @Volatile
    private var context: ProductPermissionContext? = null

    fun set(context: ProductPermissionContext) {
        this.context = context
    }

    fun get(): ProductPermissionContext? = context

    fun clear() {
        context = null
    }
}
