import Foundation
import SubstrateSdk

extension Chat {
    enum DeviceSyncChatId: Equatable {
        case contact(accountId: Data)
    }
}

// MARK: - ScaleCodable

extension Chat.DeviceSyncChatId: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        switch index {
        case 0:
            let accountId = try scaleDecoder.readAndConfirm(count: 32)
            self = .contact(accountId: accountId)
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .contact(accountId):
            try UInt8(0).encode(scaleEncoder: scaleEncoder)
            scaleEncoder.appendRaw(data: accountId)
        }
    }
}
