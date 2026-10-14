import Foundation

public protocol MessageExchangeModeProviding {
    var multideviceSignKeyIds: Set<String> { get }

    func mode(forSignKeyId signKeyId: String) -> MessageExchangeMode
}

public extension MessageExchangeModeProviding {
    func mode(for own: MessageExchange.Own) -> MessageExchangeMode {
        mode(forSignKeyId: own.signKeyId)
    }
}

public struct FixedMessageExchangeModeProvider: MessageExchangeModeProviding {
    private let mode: MessageExchangeMode

    public init(mode: MessageExchangeMode) {
        self.mode = mode
    }

    public var multideviceSignKeyIds: Set<String> {
        []
    }

    public func mode(forSignKeyId _: String) -> MessageExchangeMode {
        mode
    }
}
