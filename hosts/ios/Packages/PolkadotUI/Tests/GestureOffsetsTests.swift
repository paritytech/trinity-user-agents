import CoreGraphics
import Foundation
import Testing
@testable import PolkadotUI

struct MovePacingTests {
    private let first = Data([1])
    private let second = Data([2])
    private let third = Data([3])

    @Test("A frame sends the newest move and drops the ones before it")
    func newestPerFrame() {
        var pacing = MovePacing()
        pacing.offer(first)
        pacing.offer(second)
        pacing.offer(third)
        #expect(pacing.frame(at: 0) == third)
        #expect(pacing.dropped == 2)
        #expect(pacing.frame(at: 0.016) == nil)
    }

    @Test("No move is sent while the last one is unanswered")
    func waitsForTheAnswer() {
        var pacing = MovePacing()
        pacing.offer(first)
        #expect(pacing.frame(at: 0) == first)
        pacing.offer(second)
        #expect(pacing.frame(at: 0.016) == nil)
        #expect(pacing.frame(at: 0.033) == nil)
        pacing.answered()
        #expect(pacing.frame(at: 0.050) == second)
    }

    @Test("Moves arriving while one is unanswered never build a backlog")
    func noBacklog() {
        var pacing = MovePacing()
        pacing.offer(first)
        _ = pacing.frame(at: 0)
        for frame in 1 ... 5 {
            pacing.offer(Data([UInt8(frame)]))
            #expect(pacing.frame(at: Double(frame) * 0.016) == nil)
        }
        pacing.answered()
        #expect(pacing.frame(at: 0.1) == Data([5]))
        #expect(pacing.frame(at: 0.116) == nil)
    }

    @Test("A move unanswered for the timeout no longer holds the next one back")
    func answerTimeout() {
        var pacing = MovePacing()
        pacing.offer(first)
        _ = pacing.frame(at: 0)
        pacing.offer(second)
        #expect(pacing.frame(at: MovePacing.answerTimeout - 0.001) == nil)
        #expect(pacing.frame(at: MovePacing.answerTimeout) == second)
    }

    @Test("Reset drops the unsent move")
    func reset() {
        var pacing = MovePacing()
        pacing.offer(first)
        pacing.reset()
        #expect(pacing.frame(at: 0) == nil)
    }
}

@MainActor
struct GestureOffsetsTests {
    @Test("An offset reaches its key's channel and answers its pan")
    func setOffset() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "piece")
        var answers = 0
        channel.onAnswer = { answers += 1 }
        offsets.set(key: "piece", offset: CGPoint(x: 42, y: 0))
        #expect(channel.offset == CGPoint(x: 42, y: 0))
        #expect(offsets.channel(for: "other").offset == nil)
        #expect(answers == 1)
    }

    @Test("A tree that arrives after an offset wins")
    func treeWins() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "piece")
        var answers = 0
        channel.onAnswer = { answers += 1 }
        offsets.set(key: "piece", offset: CGPoint(x: 42, y: 0))
        offsets.treeArrived()
        #expect(channel.offset == nil)
        #expect(answers == 2)
        offsets.set(key: "piece", offset: CGPoint(x: 50, y: 0))
        #expect(channel.offset == CGPoint(x: 50, y: 0))
    }
}
