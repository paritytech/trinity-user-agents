import Foundation

public struct XcmTransferFeatures {
    public let hasDeliveryFee: Bool
    public let usesTeleports: Bool
    public let shouldUseXcmExecute: Bool

    public init(hasDeliveryFee: Bool, usesTeleports: Bool, shouldUseXcmExecute: Bool) {
        self.hasDeliveryFee = hasDeliveryFee
        self.usesTeleports = usesTeleports
        self.shouldUseXcmExecute = shouldUseXcmExecute
    }
}
