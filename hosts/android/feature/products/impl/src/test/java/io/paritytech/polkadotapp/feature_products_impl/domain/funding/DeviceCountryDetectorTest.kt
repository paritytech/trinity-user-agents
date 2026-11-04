package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import android.content.Context
import android.content.pm.PackageManager
import android.telephony.TelephonyManager
import androidx.core.content.ContextCompat
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import java.util.Locale

class DeviceCountryDetectorTest {
    private val telephony = mockk<TelephonyManager>()
    private val context = mockk<Context> {
        every { getSystemService(TelephonyManager::class.java) } returns telephony
    }
    private val defaultLocale = Locale.getDefault()

    // With no location permission granted the location is never read, which leaves the SIM, network and locale.
    @Before
    fun denyLocation() {
        mockkStatic(ContextCompat::class)
        every { ContextCompat.checkSelfPermission(any(), any()) } returns PackageManager.PERMISSION_DENIED
    }

    @After
    fun restore() {
        unmockkStatic(ContextCompat::class)
        Locale.setDefault(defaultLocale)
    }

    @Test
    fun `the SIM's country comes first`() = runTest {
        every { telephony.simCountryIso } returns "de"
        every { telephony.networkCountryIso } returns "fr"

        assertEquals("DE", detector().detect())
    }

    @Test
    fun `the network's country stands in without a SIM`() = runTest {
        every { telephony.simCountryIso } returns ""
        every { telephony.networkCountryIso } returns "fr"

        assertEquals("FR", detector().detect())
    }

    @Test
    fun `the locale's region is the last resort, and only an alpha-2 code counts`() = runTest {
        every { telephony.simCountryIso } returns "xyz"
        every { telephony.networkCountryIso } returns ""
        Locale.setDefault(Locale.Builder().setLanguage("it").setRegion("IT").build())

        assertEquals("IT", detector().detect())
    }

    private fun TestScope.detector() = DeviceCountryDetector(context, testDispatchers())
}
