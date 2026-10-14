import Foundation

public struct XcmAssetTransferFee: Decodable {
    public enum FeeType: String, Decodable {
        case proportional
        case standard
    }

    public struct Mode: Decodable {
        public let type: XcmAssetTransferFee.FeeType
        public let value: String?

        public init(type: XcmAssetTransferFee.FeeType, value: String?) {
            self.type = type
            self.value = value
        }
    }

    public let mode: XcmAssetTransferFee.Mode
    public let instructions: String

    public init(mode: XcmAssetTransferFee.Mode, instructions: String) {
        self.mode = mode
        self.instructions = instructions
    }
}
