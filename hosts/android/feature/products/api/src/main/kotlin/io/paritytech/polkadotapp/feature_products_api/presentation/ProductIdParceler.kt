package io.paritytech.polkadotapp.feature_products_api.presentation

import android.os.Parcel
import kotlinx.parcelize.Parceler
import uniffi.truapi.ProductId

object ProductIdParceler : Parceler<ProductId> {
    override fun create(parcel: Parcel): ProductId = ProductId.parse(requireNotNull(parcel.readString())).getOrThrow()

    override fun ProductId.write(parcel: Parcel, flags: Int) = parcel.writeString(value)
}
