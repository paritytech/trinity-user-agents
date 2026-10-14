import Foundation
import SubstrateSdk

extension Chat {
    struct RemoteTokenContent: Equatable {
        let token: Data
        let pushType: PushType

        enum PushType: UInt8, CaseIterable {
            case android = 0
            case ios = 1
            case iosVoIP = 2

            var platform: PeerPlatform {
                switch self {
                case .android:
                    .android
                case .ios,
                     .iosVoIP:
                    .ios
                }
            }

            var isVoIP: Bool {
                switch self {
                case .android,
                     .ios:
                    false
                case .iosVoIP:
                    true
                }
            }
        }
    }
}

extension Chat.RemoteTokenContent: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        token = try Data(scaleDecoder: scaleDecoder)
        pushType = try Chat.RemoteTokenContent.PushType(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try token.encode(scaleEncoder: scaleEncoder)
        try pushType.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.RemoteTokenContent.PushType: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let raw = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = Self(rawValue: raw) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}
