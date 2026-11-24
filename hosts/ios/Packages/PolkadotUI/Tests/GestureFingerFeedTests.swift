import CoreGraphics
import Testing
@testable import PolkadotUI

@MainActor
struct GestureFingerFeedTests {
    private final class Follower {
        var samples: [(WidgetTouchPhase, CGPoint)] = []
    }

    @Test func everyFollowerGetsEachSampleAsSent() {
        let feed = GestureFingerFeed()
        let first = Follower()
        let second = Follower()
        feed.follow(first) { first.samples.append(($0, $1)) }
        feed.follow(second) { second.samples.append(($0, $1)) }

        feed.send(.down, at: CGPoint(x: 10, y: 20))
        feed.send(.move, at: CGPoint(x: 30, y: 20))
        feed.send(.up, at: CGPoint(x: 30, y: 25))

        for follower in [first, second] {
            #expect(follower.samples.map(\.1) == [CGPoint(x: 10, y: 20), CGPoint(x: 30, y: 20), CGPoint(x: 30, y: 25)])
            #expect(follower.samples.map { "\($0.0)" } == ["down", "move", "up"])
        }
    }

    @Test func aFollowerThatLeftHearsNothingMore() {
        let feed = GestureFingerFeed()
        let follower = Follower()
        feed.follow(follower) { follower.samples.append(($0, $1)) }
        feed.send(.down, at: .zero)
        feed.unfollow(follower)
        feed.send(.move, at: CGPoint(x: 5, y: 5))
        #expect(follower.samples.count == 1)
    }

    @Test func samplesArriveOnScreenFromTheGesturesOrigin() {
        let feed = GestureFingerFeed()
        let follower = Follower()
        feed.follow(follower) { follower.samples.append(($0, $1)) }
        feed.origin = CGPoint(x: 16, y: 300)
        feed.send(.down, at: CGPoint(x: 4, y: 5))
        #expect(follower.samples.map(\.1) == [CGPoint(x: 20, y: 305)])
    }

    @Test func followingAgainReplacesTheEarlierHandler() {
        let feed = GestureFingerFeed()
        let follower = Follower()
        var earlier = 0
        feed.follow(follower) { _, _ in earlier += 1 }
        feed.follow(follower) { follower.samples.append(($0, $1)) }
        feed.send(.cancel, at: .zero)
        #expect(earlier == 0)
        #expect(follower.samples.count == 1)
    }
}
