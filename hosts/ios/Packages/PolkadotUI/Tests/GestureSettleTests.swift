import CoreGraphics
import Foundation
import Testing
@testable import PolkadotUI

struct GestureSettleTests {
    @Test("Released short of 90% of the travel, or cancelled, the thumb goes home; at or past it, to the end")
    func kind() {
        #expect(GestureSettle.kind(released: 120, travel: 200, cancelled: false) == .home)
        #expect(GestureSettle.kind(released: 180, travel: 200, cancelled: false) == .end)
        #expect(GestureSettle.kind(released: 199, travel: 200, cancelled: true) == .home)
        #expect(GestureSettle.kind(released: 10, travel: 0, cancelled: false) == .home)
    }

    @Test("The target counts from where the worker's last tree drew the thumb")
    func target() {
        let drawnAt = GestureSettle.drawnAt(answeredMove: CGPoint(x: 112.4, y: 0), answer: CGPoint(x: 112.4, y: 0))
        #expect(drawnAt == .zero)
        #expect(GestureSettle.target(.home, axis: .horizontal, travel: 200, drawnAt: drawnAt) == .zero)
        #expect(GestureSettle.target(.end, axis: .horizontal, travel: 200, drawnAt: drawnAt) == CGPoint(x: 200, y: 0))
        let moved = GestureSettle.drawnAt(answeredMove: CGPoint(x: 90.3, y: 0), answer: CGPoint(x: 50.3, y: 0))
        #expect(moved == CGPoint(x: 40, y: 0))
        #expect(GestureSettle.target(.home, axis: .horizontal, travel: 200, drawnAt: moved) == CGPoint(x: -40, y: 0))
        #expect(GestureSettle.target(.end, axis: .vertical, travel: 80, drawnAt: .zero) == CGPoint(x: 0, y: 80))
    }

    @Test("Reduced motion settles at once; otherwise a spring home and a short ease to the end")
    func animation() {
        #expect(GestureSettle.animation(.home, reduceMotion: true) == nil)
        #expect(GestureSettle.animation(.end, reduceMotion: true) == nil)
        #expect(GestureSettle.animation(.home, reduceMotion: false) != nil)
        #expect(GestureSettle.endDuration >= 0.12 && GestureSettle.endDuration <= 0.16)
    }
}

@MainActor
struct GestureSettleChannelTests {
    @Test("A settle moves the thumb, and a late answer to a move cannot pull it back before the next tree")
    func holdsUntilTheTree() {
        let offsets = GestureOffsets()
        let channel = offsets.channel(for: "thumb")
        offsets.set(key: "thumb", offset: CGPoint(x: 120, y: 0))
        channel.settle(to: .zero, animation: nil, now: 10)
        #expect(channel.offset == .zero)
        offsets.set(key: "thumb", offset: CGPoint(x: 124, y: 0))
        #expect(channel.offset == .zero)
        offsets.treeArrived()
        #expect(channel.offset == nil)
        offsets.set(key: "thumb", offset: CGPoint(x: 8, y: 0))
        #expect(channel.offset == CGPoint(x: 8, y: 0))
    }

    @Test("A pan starting during a settle ends it at once; after the settle's time it is over by itself")
    func newPanEndsTheSettle() {
        let channel = GestureOffsets().channel(for: "thumb")
        channel.settle(to: .zero, animation: nil, now: 10)
        #expect(channel.isSettling(at: 10.2))
        #expect(!channel.isSettling(at: 10 + GestureSettle.longest + 0.01))
        channel.endSettle()
        #expect(!channel.isSettling(at: 10.2))
        channel.receive(CGPoint(x: 5, y: 0))
        #expect(channel.offset == CGPoint(x: 5, y: 0))
    }

    @Test("The pacer remembers the place of the move it sent last")
    func pacerSentPlace() {
        let pacer = GestureMovePacer()
        pacer.start(send: { _ in }, offsets: nil, sendOnTouch: true, answerWait: 0)
        pacer.offer(Data([1]), place: CGPoint(x: 33.5, y: 0))
        #expect(pacer.sentPlace == CGPoint(x: 33.5, y: 0))
        pacer.offer(Data([2]), place: CGPoint(x: 40, y: 0))
        #expect(pacer.sentPlace == CGPoint(x: 33.5, y: 0))
        pacer.stop()
        #expect(pacer.sentPlace == nil)
    }
}
