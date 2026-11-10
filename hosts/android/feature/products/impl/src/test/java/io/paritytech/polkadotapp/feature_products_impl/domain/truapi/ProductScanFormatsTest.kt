package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import com.google.mlkit.vision.barcode.common.Barcode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.CodeFormat

class ProductScanFormatsTest {
    @Test
    fun `every format a product can ask for maps to ML Kit and back`() {
        for (format in CodeFormat.entries) {
            val read = ProductScanFormats.mlKitFormats(listOf(format)).map { ProductScanFormats.codeFor(it, "1") }
            assertEquals("$format", read.map { it?.first }.distinct(), listOf(format))
        }
        assertNull(ProductScanFormats.codeFor(Barcode.FORMAT_UNKNOWN, "1"))
    }

    @Test
    fun `UPC-A is read as EAN-13 with a leading zero, as on iOS`() {
        assertEquals(
            CodeFormat.EAN13 to "0012345678905",
            ProductScanFormats.codeFor(Barcode.FORMAT_UPC_A, "012345678905"),
        )
    }
}
