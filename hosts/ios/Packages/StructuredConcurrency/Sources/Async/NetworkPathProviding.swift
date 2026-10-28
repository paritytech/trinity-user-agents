import AsyncExtensions
import Foundation

public protocol NetworkPathProviding: Sendable {
    func pathStream() -> AnyAsyncSequence<Bool>
}
