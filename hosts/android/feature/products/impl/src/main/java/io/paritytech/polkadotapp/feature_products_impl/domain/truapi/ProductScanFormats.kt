package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import com.google.mlkit.vision.barcode.common.Barcode
import uniffi.truapi.CodeFormat

/** Translates between the formats a product names and the ones ML Kit reads. */
internal object ProductScanFormats {
    private val mlKitFormats = mapOf(
        CodeFormat.QR to listOf(Barcode.FORMAT_QR_CODE),
        CodeFormat.AZTEC to listOf(Barcode.FORMAT_AZTEC),
        CodeFormat.DATA_MATRIX to listOf(Barcode.FORMAT_DATA_MATRIX),
        CodeFormat.PDF417 to listOf(Barcode.FORMAT_PDF417),
        CodeFormat.EAN13 to listOf(Barcode.FORMAT_EAN_13, Barcode.FORMAT_UPC_A),
        CodeFormat.EAN8 to listOf(Barcode.FORMAT_EAN_8),
        CodeFormat.UPC_E to listOf(Barcode.FORMAT_UPC_E),
        CodeFormat.CODE128 to listOf(Barcode.FORMAT_CODE_128),
        CodeFormat.CODE39 to listOf(Barcode.FORMAT_CODE_39),
        CodeFormat.CODE93 to listOf(Barcode.FORMAT_CODE_93),
        CodeFormat.ITF to listOf(Barcode.FORMAT_ITF),
        CodeFormat.CODABAR to listOf(Barcode.FORMAT_CODABAR),
    )

    fun mlKitFormats(formats: List<CodeFormat>): List<Int> = formats.flatMap(mlKitFormats::getValue)

    /**
     * The code as a product sees it, or null for a format no product can ask for. UPC-A is read
     * as EAN-13 with a leading zero, which is how iOS reports it.
     */
    fun codeFor(mlKitFormat: Int, text: String): Pair<CodeFormat, String>? = when (mlKitFormat) {
        Barcode.FORMAT_UPC_A -> CodeFormat.EAN13 to "0$text"
        else -> mlKitFormats.entries.firstOrNull { mlKitFormat in it.value }?.let { it.key to text }
    }
}
