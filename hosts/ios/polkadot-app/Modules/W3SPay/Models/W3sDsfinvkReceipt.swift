import Foundation

/// Serial may contain spaces and hyphens — only the deeplink `id` is alphanumeric.
struct W3sDsfinvkReceipt: Equatable {
    let serial: String
    let transactionNumber: String
    let amount: W3sAmount

    var paymentId: String {
        "\(serial)/\(transactionNumber)"
    }
}
