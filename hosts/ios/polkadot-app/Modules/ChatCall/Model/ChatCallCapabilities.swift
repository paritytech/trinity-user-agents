import Foundation

struct ChatCallCapability: OptionSet {
    typealias RawValue = UInt8

    static let mute = ChatCallCapability(rawValue: 1 << 0)
    static let audioRoute = ChatCallCapability(rawValue: 1 << 1)
    static let video = ChatCallCapability(rawValue: 1 << 2)
    static let all: ChatCallCapability = [.mute, .audioRoute, .video]
    static let none: ChatCallCapability = []

    let rawValue: UInt8

    init(rawValue: RawValue) {
        self.rawValue = rawValue
    }
}
