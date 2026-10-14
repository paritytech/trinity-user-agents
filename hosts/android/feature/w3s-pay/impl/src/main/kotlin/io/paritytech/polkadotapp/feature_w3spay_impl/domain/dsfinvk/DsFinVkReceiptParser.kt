package io.paritytech.polkadotapp.feature_w3spay_impl.domain.dsfinvk

import java.math.BigDecimal

/**
 * A DSFinV-K Annex I cash-register receipt QR ("Kassenbeleg-V1"), reduced to the fields the W3S
 * payment flow needs.
 *
 * @param serialNumber the KASSEN_SERIENNUMMER
 * @param transactionNumber the TRANSAKTIONSNUMMER
 * @param amount the total payment amount (sum of the payment entries after the second `^`)
 */
class DsFinVkReceipt(
    val serialNumber: String,
    val transactionNumber: String,
    val amount: BigDecimal,
)

/**
 * Parses DSFinV-K Annex I receipt QR codes. The decoded string is `;`-separated:
 *
 * `V0;<serial>;<processType>;<processData>;<txNumber>;<sigCounter>;<start>;<log>;<sigAlg>;<timeFmt>;<sig>;<pubKey>`
 *
 * For `processType == "Kassenbeleg-V1"`, `processData` is `<Belegtyp>^<taxBreakdown>^<payments>`, and
 * the total amount is the sum of the payment entries (each `Betrag:Zahlungsart[:Währung]`) after the
 * second `^`. Returns `null` for anything that is not a well-formed Kassenbeleg-V1 receipt.
 */
object DsFinVkReceiptParser {
    private const val PREFIX = "V0;"
    private const val VERSION = "V0"
    private const val EXPECTED_PROCESS_TYPE = "Kassenbeleg-V1"

    // Standard DSFinV-K Annex I QR layout has 12 `;`-separated fields.
    private const val MIN_FIELDS = 12

    private const val INDEX_SERIAL = 1
    private const val INDEX_PROCESS_TYPE = 2
    private const val INDEX_PROCESS_DATA = 3
    private const val INDEX_TRANSACTION_NUMBER = 4

    fun parse(decoded: String): Result<DsFinVkReceipt> = runCatching {
        require(decoded.startsWith(PREFIX)) { "Not a DSFinV-K $VERSION QR code" }

        val parts = decoded.split(";")
        require(parts.size >= MIN_FIELDS) { "Expected at least $MIN_FIELDS fields, got ${parts.size}" }
        require(parts[0] == VERSION) { "Unsupported version '${parts[0]}'" }
        require(parts[INDEX_PROCESS_TYPE] == EXPECTED_PROCESS_TYPE) {
            "Unsupported process type '${parts[INDEX_PROCESS_TYPE]}'"
        }

        val serialNumber = parts[INDEX_SERIAL]
        require(serialNumber.isNotEmpty()) { "Empty cash register serial number" }
        val transactionNumber = parts[INDEX_TRANSACTION_NUMBER]
        require(transactionNumber.isNotEmpty()) { "Empty transaction number" }

        DsFinVkReceipt(
            serialNumber = serialNumber,
            transactionNumber = transactionNumber,
            amount = parseAmount(parts[INDEX_PROCESS_DATA]),
        )
    }

    private fun parseAmount(processData: String): BigDecimal {
        val segments = processData.split("^")
        require(segments.size >= 3) { "Unexpected processData format" }

        // The payments segment is everything after the second `^`.
        val payments = segments.drop(2).joinToString("^")

        return payments.split("_").fold(BigDecimal.ZERO) { total, entry ->
            val value = requireNotNull(entry.substringBefore(":").toBigDecimalOrNull()) {
                "Cannot parse payment amount from '$entry'"
            }
            total + value
        }
    }
}
