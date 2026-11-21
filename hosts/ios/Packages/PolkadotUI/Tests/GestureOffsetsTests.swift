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

    @Test("A move goes out at its touch when nothing is unanswered, at most one per frame")
    func sendsAtTheTouch() {
        var pacing = MovePacing()
        pacing.offer(first)
        #expect(pacing.touch(at: 0) == first)
        pacing.offer(second)
        #expect(pacing.touch(at: 0.001) == nil)
        pacing.answered()
        #expect(pacing.touch(at: 0.002) == nil)
        #expect(pacing.frame(at: 0.003) == nil)
        #expect(pacing.touch(at: 0.017) == second)
    }

    @Test("A frame sends the move its touch could not, once the answer is in")
    func frameSendsWhatTheTouchHeld() {
        var pacing = MovePacing()
        pacing.offer(first)
        _ = pacing.frame(at: 0)
        pacing.offer(second)
        #expect(pacing.touch(at: 0.010) == nil)
        pacing.answered()
        #expect(pacing.frame(at: 0.016) == second)
        pacing.offer(third)
        #expect(pacing.touch(at: 0.017) == nil)
    }

    @Test("A pan's second move goes out one refresh after its first, with no display-link callback in between")
    func secondMoveWithoutACallback() {
        var pacing = MovePacing()
        pacing.offer(first)
        #expect(pacing.touch(at: 0) == first)
        pacing.answered()
        pacing.offer(second)
        #expect(pacing.touch(at: 0.0167) == second)
    }

    @Test("One move per refresh follows the screen's refresh rate")
    func refreshRate() {
        #expect(abs(MovePacing(refresh: 1.0 / 120).minimumGap - 0.005) < 1e-9)
        #expect(abs(MovePacing().minimumGap - 0.01) < 1e-9)
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
        let channel = offsets.channel(for: "thumb")
        var answers = 0
        channel.onAnswer = { answers += 1 }
        offsets.set(key: "thumb", offset: CGPoint(x: 42, y: 0))
        #expect(channel.offset == CGPoint(x: 42, y: 0))
        #expect(offsets.channel(for: "other").offset == nil)
        #expect(answers == 1)
    }

    @Test("A tree that arrives after an offset wins")
    func treeWins() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "thumb")
        var answers = 0
        channel.onAnswer = { answers += 1 }
        offsets.set(key: "thumb", offset: CGPoint(x: 42, y: 0))
        offsets.treeArrived()
        #expect(channel.offset == nil)
        #expect(answers == 2)
        offsets.set(key: "thumb", offset: CGPoint(x: 50, y: 0))
        #expect(channel.offset == CGPoint(x: 50, y: 0))
    }
}

struct GesturePacingSettingsTests {
    @Test("Moves go out at the touch unless the setting says no")
    func sendOnTouch() {
        #expect(GesturePacingSettings.sendOnTouch(setting: nil))
        #expect(GesturePacingSettings.sendOnTouch(setting: true))
        #expect(!GesturePacingSettings.sendOnTouch(setting: false))
        #expect(!GesturePacingSettings.sendOnTouch(setting: NSNumber(value: 0)))
    }

    @Test("The wait for an answer is 4 ms by default, 0 turns it off, and it never takes most of a frame")
    func answerWait() {
        #expect(GesturePacingSettings.answerWait(setting: nil) == 0.004)
        #expect(GesturePacingSettings.answerWait(setting: NSNumber(value: 0)) == 0)
        #expect(GesturePacingSettings.answerWait(setting: NSNumber(value: 2)) == 0.002)
        #expect(GesturePacingSettings.answerWait(setting: "3") == 0.003)
        #expect(GesturePacingSettings.answerWait(setting: NSNumber(value: 50)) == GesturePacingSettings.longestWait)
        #expect(GesturePacingSettings.answerWait(setting: NSNumber(value: -1)) == 0)
        #expect(GesturePacingSettings.answerWait(setting: "soon") == GesturePacingSettings.defaultWait)
    }
}

struct GestureAnswerMailboxTests {
    private let thumb = GestureAnswer(key: "thumb", offset: CGPoint(x: 12.5, y: 0))

    @Test("A wait ends as soon as an answer arrives from another thread")
    func wakesOnDelivery() {
        let mailbox = GestureAnswerMailbox()
        let answer = thumb
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.005) { mailbox.deliver(answer) }
        let start = Date()
        #expect(mailbox.wait(after: 0, until: Date(timeIntervalSinceNow: 1)))
        #expect(Date().timeIntervalSince(start) < 0.5)
        #expect(mailbox.take() == thumb)
    }

    @Test("A wait with no answer gives up at its deadline")
    func timesOut() {
        let mailbox = GestureAnswerMailbox()
        mailbox.deliver(thumb)
        #expect(!mailbox.wait(after: 1, until: Date(timeIntervalSinceNow: 0.003)))
    }

    @Test("Only the newest answer is kept, and it is taken once")
    func newestOnce() {
        let mailbox = GestureAnswerMailbox()
        mailbox.deliver(GestureAnswer(key: "thumb", offset: CGPoint(x: 1, y: 0)))
        mailbox.deliver(thumb)
        #expect(mailbox.delivered == 2)
        #expect(mailbox.take() == thumb)
        #expect(mailbox.take() == nil)
    }

    @Test("Every batch of cleared cells is kept, in order, and a wait wakes on one")
    func cellsAddUp() {
        let mailbox = GestureAnswerMailbox()
        DispatchQueue.global().asyncAfter(deadline: .now() + 0.005) {
            mailbox.deliver(CoverAnswer(key: "foil", cells: [1, 2]))
        }
        #expect(mailbox.wait(after: 0, until: Date(timeIntervalSinceNow: 1)))
        mailbox.deliver(CoverAnswer(key: "foil", cells: [3]))
        #expect(mailbox.delivered == 2)
        #expect(mailbox.takeCells() == [CoverAnswer(key: "foil", cells: [1, 2]), CoverAnswer(key: "foil", cells: [3])])
        #expect(mailbox.takeCells().isEmpty)
        #expect(mailbox.take() == nil, "cells are not an offset")
    }
}

@MainActor
struct GestureOffsetsDeliveryTests {
    @Test("A delivered offset is applied once, whoever applies it first")
    func appliedOnce() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "thumb")
        var answers = 0
        channel.onAnswer = { answers += 1 }
        offsets.deliver(key: "thumb", offset: CGPoint(x: 7.25, y: 0))
        offsets.applyDelivered()
        offsets.applyDelivered()
        #expect(channel.offset == CGPoint(x: 7.25, y: 0))
        #expect(answers == 1)
    }

    @Test("A tree drops an offset delivered before it and not yet applied")
    func treeDropsDelivered() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "thumb")
        offsets.deliver(key: "thumb", offset: CGPoint(x: 9, y: 0))
        offsets.treeArrived()
        offsets.applyDelivered()
        #expect(channel.offset == nil)
    }

    @Test("A pacer that sends a move waits for its answer and draws it in the same frame")
    func pacerWaitsForTheAnswer() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "thumb")
        let pacer = GestureMovePacer()
        var sent: [Data] = []
        pacer.start(
            send: { payload in
                sent.append(payload)
                DispatchQueue.global().asyncAfter(deadline: .now() + 0.001) {
                    offsets.deliver(key: "thumb", offset: CGPoint(x: 3, y: 0))
                }
            },
            offsets: offsets,
            sendOnTouch: true,
            answerWait: 0.5
        )
        channel.onAnswer = { [pacer] in pacer.answered() }
        pacer.offer(Data([1]))
        #expect(sent == [Data([1])])
        #expect(channel.offset == CGPoint(x: 3, y: 0))
        #expect(pacer.pacing.sentAt == nil)
        pacer.stop()
    }

    @Test("Delivered cells are applied once, whoever applies them first, and answer the pan")
    func cellsAppliedOnce() {
        let offsets = GestureOffsets()
        let pan = offsets.channel(for: "scratch")
        var answers = 0
        pan.onAnswer = { answers += 1 }
        offsets.deliver(key: "foil", cells: [3, 4])
        offsets.deliver(key: "foil", cells: [5])
        offsets.applyDelivered()
        offsets.applyDelivered()
        #expect(offsets.cover(for: "foil").cleared?.count == 3)
        #expect(pan.offset == nil, "clearing cells moves no gesture")
        #expect(answers == 2)
    }

    @Test("A tree drops cells delivered before it and not yet applied")
    func treeDropsDeliveredCells() {
        let offsets = GestureOffsets()
        offsets.deliver(key: "foil", cells: [1, 2])
        offsets.treeArrived()
        offsets.applyDelivered()
        #expect(offsets.cover(for: "foil").cleared == nil)
    }

    @Test("A cover grid's pan waits for its cleared cells and draws them in the frame that sent the move")
    func pacerWaitsForCells() {
        let offsets = GestureOffsets()
        let pan = offsets.channel(for: "scratch")
        let pacer = GestureMovePacer()
        pacer.start(
            send: { _ in
                DispatchQueue.global().asyncAfter(deadline: .now() + 0.001) {
                    offsets.deliver(key: "foil", cells: [7, 8])
                }
            },
            offsets: offsets,
            sendOnTouch: true,
            answerWait: 0.5
        )
        pan.onAnswer = { [pacer] in pacer.answered() }
        pacer.offer(Data([1]))
        #expect(offsets.cover(for: "foil").cleared?.count == 2)
        #expect(pacer.pacing.sentAt == nil)
        pacer.stop()
    }

    @Test("With no wait and no send at the touch, the pacer leaves the move for the display frame")
    func pacerWithoutTheFixes() {
        let offsets = GestureOffsets()
        let pacer = GestureMovePacer()
        var sent = 0
        pacer.start(send: { _ in sent += 1 }, offsets: offsets, sendOnTouch: false, answerWait: 0)
        pacer.offer(Data([1]))
        #expect(sent == 0)
        #expect(pacer.pacing.pending == Data([1]))
        pacer.stop()
    }
}
