import SubstrateSdk

extension CoinagePallet {
    enum AliasState: Decodable, Equatable {
        case locked(LockInfo)
        case unloaded

        init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let state = try container.decode(String.self)

            switch state {
            case "Locked":
                let lockInfo = try container.decode(LockInfo.self)
                self = .locked(lockInfo)
            case "Unloaded":
                self = .unloaded
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unsupported alias state: \(state)"
                )
            }
        }
    }

    struct LockInfo: Decodable, Equatable {
        @StringCodable var retries: UInt8
        @StringCodable var until: UInt64
    }
}
