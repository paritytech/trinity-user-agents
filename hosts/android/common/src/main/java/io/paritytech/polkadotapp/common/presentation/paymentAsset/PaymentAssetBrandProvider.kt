package io.paritytech.polkadotapp.common.presentation.paymentAsset

import kotlinx.coroutines.flow.StateFlow

interface PaymentAssetBrandProvider {
    /**
     * The bundled brand until Remote Config has synced, then the published symbol at once and each logo as
     * [PaymentAssetLogo.Remote] once it has been fetched. A logo that is not published or fails to load stays
     * [PaymentAssetLogo.Bundled].
     */
    val brand: StateFlow<PaymentAssetBrand>
}
