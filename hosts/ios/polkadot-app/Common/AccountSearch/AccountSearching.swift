import Foundation
import AsyncExtensions

protocol AccountSearching<RecentPayload, MatchPayload>: AnyObject {
    associatedtype RecentPayload
    associatedtype MatchPayload

    func setup()
    func sourcesChanged() -> AnyAsyncSequence<Void>
    func searchPhases(
        query: String?
    ) -> AsyncThrowingStream<AccountSearchSections<RecentPayload, MatchPayload>, Error>
}
