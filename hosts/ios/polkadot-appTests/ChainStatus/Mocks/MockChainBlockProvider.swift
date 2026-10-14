import Foundation
import AsyncExtensions
@testable import polkadot_app

actor MockChainBlockProvider: ChainBlockProviding {
    private let subject = AsyncCurrentValueSubject<[ChainConnectionTarget: ChainBlockInfo]>([:])

    nonisolated func blockStream() -> AnyAsyncSequence<[ChainConnectionTarget: ChainBlockInfo]> {
        subject.eraseToAnyAsyncSequence()
    }

    func setActive(_: Bool) {}

    func clear(for _: ChainConnectionTarget) {}

    func simulateBlockUpdate(_ blocks: [ChainConnectionTarget: ChainBlockInfo]) {
        subject.send(blocks)
    }
}
