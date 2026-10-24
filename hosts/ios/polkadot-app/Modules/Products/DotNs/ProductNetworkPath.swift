import AsyncExtensions
import Foundation
import Products

struct ProductNetworkPath: ProductNetworkPathProviding {
    func pathStream() -> AnyAsyncSequence<Bool> {
        NetworkPathMonitor().pathStream()
    }
}
