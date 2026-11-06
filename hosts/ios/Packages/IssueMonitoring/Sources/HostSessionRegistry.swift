import Foundation
import os

public final class HostSessionRegistry: Sendable {
    public static let shared = HostSessionRegistry()

    private let sessions = OSAllocatedUnfairLock(initialState: [String: UUID]())

    public init() {}

    public func open(productID: String, sessionID: UUID) {
        sessions.withLock { $0[productID] = sessionID }
    }

    public func close(productID: String, sessionID: UUID) {
        sessions.withLock { sessions in
            guard sessions[productID] == sessionID else { return }
            sessions[productID] = nil
        }
    }

    public func sessionID(forProductOrSubname name: String) -> UUID? {
        sessions.withLock { sessions in
            sessions[name] ?? sessions
                .filter { name.hasSuffix(".\($0.key)") }
                .max { $0.key.count < $1.key.count }?
                .value
        }
    }
}
