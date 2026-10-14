import Foundation
import HandoffService
import AsyncExtensions

@testable import polkadot_app

enum MockHopNodes {
    static let trusted: ChatRemoteMessageContent.NodeEndpoint = .wssUrl("wss://trusted.test")
    static let untrusted: ChatRemoteMessageContent.NodeEndpoint = .wssUrl("wss://untrusted.test")
}

final class MockHOPNodeProvider: HOPNodeProviding {
    let allowedNodes: Set<ChatRemoteMessageContent.NodeEndpoint>

    init(allowedNodes: Set<ChatRemoteMessageContent.NodeEndpoint> = [MockHopNodes.trusted]) {
        self.allowedNodes = allowedNodes
    }

    func selectNode() -> ChatRemoteMessageContent.NodeEndpoint? {
        allowedNodes.first
    }

    func isNodeAllowed(_ node: ChatRemoteMessageContent.NodeEndpoint) -> Bool {
        allowedNodes.contains(node)
    }
}
