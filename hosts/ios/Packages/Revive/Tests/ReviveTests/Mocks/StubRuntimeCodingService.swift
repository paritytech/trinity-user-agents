import Foundation
import Operation_iOS
import SubstrateSdk

/// A runtime whose coder factory never materialises: the stubbed request factory fails first.
final class StubRuntimeCodingService: RuntimeCodingServiceProtocol {
    func fetchCoderFactoryOperation() -> BaseOperation<RuntimeCoderFactoryProtocol> {
        ClosureOperation { throw StubError.unused }
    }
}
