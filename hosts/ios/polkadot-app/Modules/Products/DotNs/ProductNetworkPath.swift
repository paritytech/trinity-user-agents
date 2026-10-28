import AsyncExtensions
import Foundation
import StructuredConcurrency

struct ProductNetworkPath: NetworkPathProviding {
    func pathStream() -> AnyAsyncSequence<Bool> {
        NetworkPathMonitor().pathStream()
    }
}
