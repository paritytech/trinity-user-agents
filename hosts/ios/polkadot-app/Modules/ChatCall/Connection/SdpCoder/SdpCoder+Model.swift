import Foundation
import SubstrateSdk
import BigInt

extension SdpCoder {
    enum SdpType: UInt8 {
        case offer
        case answer
    }

    struct MinimalSetup {
        let sdpType: SdpType
        let sessionId: BigUInt // compact int
        let sessionVersion: BigUInt // compact int
        let iceUfrag: String
        let icePwd: String
        let fingerprint: Data
        let candidates: [MinimalCandidate]
    }

    struct IP4Address {
        let comp1: UInt8
        let comp2: UInt8
        let comp3: UInt8
        let comp4: UInt8

        init?(fromString: String) {
            let parts = fromString.split(separator: ".")
            guard
                parts.count == 4,
                let comp1 = UInt8(parts[0]),
                let comp2 = UInt8(parts[1]),
                let comp3 = UInt8(parts[2]),
                let comp4 = UInt8(parts[3]) else {
                return nil
            }

            self.comp1 = comp1
            self.comp2 = comp2
            self.comp3 = comp3
            self.comp4 = comp4
        }

        func toString() -> String {
            "\(comp1).\(comp2).\(comp3).\(comp4)"
        }
    }

    struct IP6Address {
        let comp1: UInt16
        let comp2: UInt16
        let comp3: UInt16
        let comp4: UInt16
        let comp5: UInt16
        let comp6: UInt16
        let comp7: UInt16
        let comp8: UInt16

        init?(inputString: String) {
            guard let values = IPV6Parser.parse(inputString) else {
                return nil
            }

            comp1 = values[0]
            comp2 = values[1]
            comp3 = values[2]
            comp4 = values[3]
            comp5 = values[4]
            comp6 = values[5]
            comp7 = values[6]
            comp8 = values[7]
        }

        func toString() -> String {
            IPV6Parser.format(
                [
                    comp1,
                    comp2,
                    comp3,
                    comp4,
                    comp5,
                    comp6,
                    comp7,
                    comp8
                ]
            )
        }
    }

    struct MinimalCandidate {
        enum TransportType: UInt8 {
            case tcp
            case udp
        }

        enum CandidateType: UInt8 {
            case host
            case srflx
            case relay
            case prflx
        }

        enum IPAddress {
            case ipv4(IP4Address)
            case ipv6(IP6Address)
        }

        let foundation: String
        let priority: UInt32
        let transportType: TransportType
        let address: IPAddress
        let port: UInt16
        let candidateType: CandidateType
    }
}

extension SdpCoder.SdpType: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension SdpCoder.MinimalSetup: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        sdpType = try SdpCoder.SdpType(scaleDecoder: scaleDecoder)
        sessionId = try BigUInt(scaleDecoder: scaleDecoder)
        sessionVersion = try BigUInt(scaleDecoder: scaleDecoder)
        iceUfrag = try String(scaleDecoder: scaleDecoder)
        icePwd = try String(scaleDecoder: scaleDecoder)
        fingerprint = try Data(scaleDecoder: scaleDecoder)
        candidates = try [SdpCoder.MinimalCandidate](scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try sdpType.encode(scaleEncoder: scaleEncoder)
        try sessionId.encode(scaleEncoder: scaleEncoder)
        try sessionVersion.encode(scaleEncoder: scaleEncoder)
        try iceUfrag.encode(scaleEncoder: scaleEncoder)
        try icePwd.encode(scaleEncoder: scaleEncoder)
        try fingerprint.encode(scaleEncoder: scaleEncoder)
        try candidates.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - TransportType ScaleCodable

extension SdpCoder.MinimalCandidate.TransportType: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - CandidateType ScaleCodable

extension SdpCoder.MinimalCandidate.CandidateType: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - IPAddress ScaleCodable

extension SdpCoder.MinimalCandidate.IPAddress: ScaleCodable {
    enum ScaleIndex: UInt8 {
        case ipv4
        case ipv6
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        guard let scaleIndex = ScaleIndex(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }

        switch scaleIndex {
        case .ipv4:
            let address = try SdpCoder.IP4Address(scaleDecoder: scaleDecoder)
            self = .ipv4(address)
        case .ipv6:
            let address = try SdpCoder.IP6Address(scaleDecoder: scaleDecoder)
            self = .ipv6(address)
        }
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        switch self {
        case let .ipv4(address):
            try ScaleIndex.ipv4.rawValue.encode(scaleEncoder: scaleEncoder)
            try address.encode(scaleEncoder: scaleEncoder)
        case let .ipv6(address):
            try ScaleIndex.ipv6.rawValue.encode(scaleEncoder: scaleEncoder)
            try address.encode(scaleEncoder: scaleEncoder)
        }
    }
}

// MARK: - MinimalCandidate ScaleCodable

extension SdpCoder.MinimalCandidate: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        foundation = try String(scaleDecoder: scaleDecoder)
        priority = try UInt32(scaleDecoder: scaleDecoder)
        transportType = try TransportType(scaleDecoder: scaleDecoder)
        address = try IPAddress(scaleDecoder: scaleDecoder)
        port = try UInt16(scaleDecoder: scaleDecoder)
        candidateType = try CandidateType(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try foundation.encode(scaleEncoder: scaleEncoder)
        try priority.encode(scaleEncoder: scaleEncoder)
        try transportType.encode(scaleEncoder: scaleEncoder)
        try address.encode(scaleEncoder: scaleEncoder)
        try port.encode(scaleEncoder: scaleEncoder)
        try candidateType.encode(scaleEncoder: scaleEncoder)
    }
}

extension SdpCoder.IP6Address: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        comp1 = try UInt16(scaleDecoder: scaleDecoder)
        comp2 = try UInt16(scaleDecoder: scaleDecoder)
        comp3 = try UInt16(scaleDecoder: scaleDecoder)
        comp4 = try UInt16(scaleDecoder: scaleDecoder)
        comp5 = try UInt16(scaleDecoder: scaleDecoder)
        comp6 = try UInt16(scaleDecoder: scaleDecoder)
        comp7 = try UInt16(scaleDecoder: scaleDecoder)
        comp8 = try UInt16(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try comp1.encode(scaleEncoder: scaleEncoder)
        try comp2.encode(scaleEncoder: scaleEncoder)
        try comp3.encode(scaleEncoder: scaleEncoder)
        try comp4.encode(scaleEncoder: scaleEncoder)
        try comp5.encode(scaleEncoder: scaleEncoder)
        try comp6.encode(scaleEncoder: scaleEncoder)
        try comp7.encode(scaleEncoder: scaleEncoder)
        try comp8.encode(scaleEncoder: scaleEncoder)
    }
}

extension SdpCoder.IP4Address: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        comp1 = try UInt8(scaleDecoder: scaleDecoder)
        comp2 = try UInt8(scaleDecoder: scaleDecoder)
        comp3 = try UInt8(scaleDecoder: scaleDecoder)
        comp4 = try UInt8(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try comp1.encode(scaleEncoder: scaleEncoder)
        try comp2.encode(scaleEncoder: scaleEncoder)
        try comp3.encode(scaleEncoder: scaleEncoder)
        try comp4.encode(scaleEncoder: scaleEncoder)
    }
}
