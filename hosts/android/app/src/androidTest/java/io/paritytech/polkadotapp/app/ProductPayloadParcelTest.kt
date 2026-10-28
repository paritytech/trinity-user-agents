package io.paritytech.polkadotapp.app

import android.os.Parcel
import android.os.Parcelable
import androidx.core.os.ParcelCompat
import io.paritytech.polkadotapp.feature_products_api.presentation.PocketAddCardPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductSettingsPayload
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.VideoGameNotificationType
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductPayloadParcelTest {
    @Test
    fun productIdentitySurvivesNavigationAndNotificationParcels() {
        val product = ProductId.parse("localhost:3000").getOrThrow()

        val restored = listOf(
            roundTrip(SpaBrowserPayload.ByProductId(product)).productId,
            roundTrip(ProductSettingsPayload(product)).productId,
            roundTrip(PocketAddCardPayload(product, "balance")).productId,
            roundTrip(VideoGameNotificationType.ProductGameStartsSoon(product)).productId,
        )

        assertEquals(List(4) { product }, restored)
    }

    private inline fun <reified T : Parcelable> roundTrip(value: T): T {
        val parcel = Parcel.obtain()
        return try {
            parcel.writeParcelable(value, 0)
            parcel.setDataPosition(0)
            requireNotNull(ParcelCompat.readParcelable(parcel, T::class.java.classLoader, T::class.java))
        } finally {
            parcel.recycle()
        }
    }
}
