import Foundation
import SubstrateSdk

struct PeerConnectionCandidate: Equatable {
    let sdp: String
    let sdpMLineIndex: UInt32
    let sdpMid: String?
}

enum PeerConnectionSignal: Equatable {
    static let offerIndex: UInt8 = 0
    static let answerIndex: UInt8 = 1
    static let candidatesIndex: UInt8 = 2
    static let closedIndex: UInt8 = 3

    case offer(String)
    case answer(String)
    case candidates([PeerConnectionCandidate])
    case closed

    var isSetup: Bool {
        switch self {
        case .offer,
             .answer:
            true
        case .candidates,
             .closed:
            false
        }
    }
}

extension PeerConnectionCandidate: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        sdp = try String(scaleDecoder: scaleDecoder)
        sdpMLineIndex = try UInt32(scaleDecoder: scaleDecoder)
        sdpMid = try ScaleOption<String>(scaleDecoder: scaleDecoder).value
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        try sdp.encode(scaleEncoder: scaleEncoder)
        try sdpMLineIndex.encode(scaleEncoder: scaleEncoder)
        try ScaleOption(value: sdpMid).encode(scaleEncoder: scaleEncoder)
    }
}

extension PeerConnectionSignal: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)

        switch index {
        case Self.offerIndex:
            let sdp = try String(scaleDecoder: scaleDecoder)
            self = .offer(sdp)
        case Self.answerIndex:
            let sdp = try String(scaleDecoder: scaleDecoder)
            self = .answer(sdp)
        case Self.candidatesIndex:
            let candidates = try [PeerConnectionCandidate](scaleDecoder: scaleDecoder)
            self = .candidates(candidates)
        case Self.closedIndex:
            self = .closed
        default:
            throw ScaleCodingError.unexpectedDecodedValue
        }
    }

    func encode(scaleEncoder: ScaleEncoding) throws {
        switch self {
        case let .offer(sdp):
            try Self.offerIndex.encode(scaleEncoder: scaleEncoder)
            try sdp.encode(scaleEncoder: scaleEncoder)
        case let .answer(sdp):
            try Self.answerIndex.encode(scaleEncoder: scaleEncoder)
            try sdp.encode(scaleEncoder: scaleEncoder)
        case let .candidates(candidates):
            try Self.candidatesIndex.encode(scaleEncoder: scaleEncoder)
            try candidates.encode(scaleEncoder: scaleEncoder)
        case .closed:
            try Self.closedIndex.encode(scaleEncoder: scaleEncoder)
        }
    }
}
