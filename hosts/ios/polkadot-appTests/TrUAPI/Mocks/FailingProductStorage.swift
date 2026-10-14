import Foundation
@testable import polkadot_app

struct StubStorageFailure: Error {}

final class FailingProductStorage: TrUAPILocalStoring {
    func read(key _: String) throws -> Data? { throw StubStorageFailure() }
    func write(key _: String, value _: Data) throws { throw StubStorageFailure() }
    func clear(key _: String) throws { throw StubStorageFailure() }
}
