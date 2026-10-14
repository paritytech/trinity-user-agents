import Foundation
import UniqueDevice

@testable import polkadot_app

final class MockJWTTokenManager: JWTTokenManaging {
    func setup(authProvider _: AppAttestProviding) {}

    func prewarm() {}
}
