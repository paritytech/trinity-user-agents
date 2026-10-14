import Foundation
import Network
import AsyncExtensions
import AsyncAlgorithms

protocol NetworkPathMonitoring: AnyObject {
    func pathStream() -> AnyAsyncSequence<Bool>
}

final class NetworkPathMonitor: NetworkPathMonitoring {
    func pathStream() -> AnyAsyncSequence<Bool> {
        NWPathMonitor()
            .map { $0.status == .satisfied }
            .removeDuplicates()
            .eraseToAnyAsyncSequence()
    }
}
