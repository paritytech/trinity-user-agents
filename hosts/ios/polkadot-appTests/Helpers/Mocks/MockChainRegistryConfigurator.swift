import Foundation
import ChainRegistry

@testable import polkadot_app

final class MockChainRegistryConfigurator: ChainRegistryConfiguring {
    func set(chainRegistry _: ChainRegistryProtocol) {}
}
