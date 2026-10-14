import Foundation

protocol W3sDsfinvkReceiptParsing {
    func tryParse(_ code: String) -> W3sDsfinvkReceipt?
}

struct W3sDsfinvkReceiptParser: W3sDsfinvkReceiptParsing {
    func tryParse(_ code: String) -> W3sDsfinvkReceipt? {
        guard code.hasPrefix(Constants.prefix) else { return nil }

        let fields = code.split(separator: Constants.fieldSeparator, omittingEmptySubsequences: false)
            .map(String.init)
        guard fields.count >= Constants.minimumFieldCount, fields[0] == Constants.version else {
            return nil
        }

        let serial = fields[1]
        let processType = fields[2]
        let processData = fields[3]
        let transactionNumber = fields[4]

        guard processType == Constants.expectedProcessType,
              !serial.isEmpty,
              !transactionNumber.isEmpty,
              let amount = parseAmount(from: processData)
        else {
            return nil
        }

        return W3sDsfinvkReceipt(
            serial: serial,
            transactionNumber: transactionNumber,
            amount: amount
        )
    }
}

private extension W3sDsfinvkReceiptParser {
    enum Constants {
        static let prefix = "V0;"
        static let version = "V0"
        static let fieldSeparator: Character = ";"
        static let processDataSeparator: Character = "^"
        static let paymentsSeparator: Character = "_"
        static let paymentFieldSeparator: Character = ":"
        static let expectedProcessType = "Kassenbeleg-V1"
        static let minimumFieldCount = 12
        static let processDataMinimumParts = 3
    }

    // processData for Kassenbeleg-V1 is `Belegtyp^tax-breakdown^payments`; total
    // is the sum of `amount:type[:currency]` entries (split by `_`) in payments.
    func parseAmount(from processData: String) -> W3sAmount? {
        let parts = processData.split(
            separator: Constants.processDataSeparator,
            omittingEmptySubsequences: false
        )
        .map(String.init)
        guard parts.count >= Constants.processDataMinimumParts else { return nil }

        let payments = parts[2]
        let entries = payments.split(separator: Constants.paymentsSeparator).map(String.init)
        guard !entries.isEmpty else { return nil }

        var total: Decimal = 0
        for entry in entries {
            guard let separatorIndex = entry.firstIndex(of: Constants.paymentFieldSeparator) else {
                return nil
            }
            let amountString = String(entry[..<separatorIndex])
            guard let parsed = W3sAmount.parse(amountString) else { return nil }
            total += parsed.decimal
        }

        return W3sAmount.fromValidatedDecimal(total)
    }
}
