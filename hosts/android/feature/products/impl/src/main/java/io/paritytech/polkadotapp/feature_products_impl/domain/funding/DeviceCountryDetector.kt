package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.LocationManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import java.util.Locale
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds

/**
 * The country the user is likely in, as ISO 3166-1 alpha-2, for the core's funding quotes: the device's last
 * known location when a location permission is already granted, then its SIM or network country, then its
 * locale's region. It never asks for a permission, and gives up on a slow location lookup rather than hold up the
 * funding sheet that waits on it.
 */
class DeviceCountryDetector @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val dispatchers: CoroutineDispatchers,
) {
    suspend fun detect(): String? = withContext(dispatchers.io) {
        locationCountry() ?: telephonyCountry() ?: Locale.getDefault().country.asAlpha2()
    }

    // The permission is checked just above the read, which lint cannot follow.
    @SuppressLint("MissingPermission")
    private suspend fun locationCountry(): String? {
        val granted = LOCATION_PERMISSIONS.any {
            ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
        }
        if (!granted || !Geocoder.isPresent()) return null

        val manager = context.getSystemService(LocationManager::class.java) ?: return null
        val location = runCatching {
            manager.getProviders(true).mapNotNull(manager::getLastKnownLocation).maxByOrNull { it.time }
        }.getOrNull() ?: return null

        // The blocking lookup ignores cancellation, so it runs on its own and only its answer is waited for.
        val lookup = CoroutineScope(dispatchers.io).async {
            runCatching {
                // The listener form needs API 33; this runs off the main thread, where the blocking form is fine.
                @Suppress("DEPRECATION")
                Geocoder(context, Locale.ROOT).getFromLocation(location.latitude, location.longitude, 1)
                    ?.firstOrNull()
                    ?.countryCode
            }.getOrNull()
        }
        return withTimeoutOrNull(GEOCODE_TIMEOUT) { lookup.await() }.asAlpha2()
    }

    private fun telephonyCountry(): String? {
        val telephony = context.getSystemService(TelephonyManager::class.java) ?: return null
        return telephony.simCountryIso.asAlpha2() ?: telephony.networkCountryIso.asAlpha2()
    }

    private fun String?.asAlpha2(): String? =
        this?.uppercase(Locale.ROOT)?.takeIf { code -> code.length == 2 && code.all { it in 'A'..'Z' } }

    private companion object {
        val GEOCODE_TIMEOUT = 1_500.milliseconds
        val LOCATION_PERMISSIONS = listOf(Manifest.permission.ACCESS_COARSE_LOCATION, Manifest.permission.ACCESS_FINE_LOCATION)
    }
}
