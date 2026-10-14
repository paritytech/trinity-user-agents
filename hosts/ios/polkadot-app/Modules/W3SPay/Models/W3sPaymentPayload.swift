import Foundation
import SubstrateSdk

/// Field order is part of the cross-system contract with the merchant — do not reorder.
struct W3sPaymentPayload: ScaleEncodable {
    let amount: String
    let timestampMs: UInt64
    let coins: [Data]
    let paymentId: String

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try amount.encode(scaleEncoder: scaleEncoder)
        try timestampMs.encode(scaleEncoder: scaleEncoder)
        try coins.encode(scaleEncoder: scaleEncoder)
        try paymentId.encode(scaleEncoder: scaleEncoder)
    }
}
