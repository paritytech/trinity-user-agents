import CoreGraphics
import SwiftUI
import Testing
@testable import PolkadotUI

struct WidgetNodeStyleTests {
    @Test("The text fades as the existing slider's does: 1 - 1.4 × progress, gone from five sevenths on")
    func textFade() {
        #expect(SlideStyle.textOpacity(progress: 0) == 1)
        #expect(abs(SlideStyle.textOpacity(progress: 0.5) - 0.3) < 0.0001)
        #expect(SlideStyle.textOpacity(progress: 5.0 / 7) < 0.0001)
        #expect(SlideStyle.textOpacity(progress: 1.2) == 0)
        #expect(SlideStyle.textOpacity(progress: -0.3) == 1)
    }

    @Test("The band sweeps from lead before the text to lead past it, then rests")
    func shimmerSweep() {
        let shimmer = SlideStyle.shimmer
        // A 160-wide line: (160 + 140) / 212 s of sweep, then 1 s at rest.
        #expect(TextShimmer<EmptyView>.center(shimmer, width: 160, elapsed: 0) == -70)
        let sweep = (160.0 + 140) / 212
        #expect(abs((TextShimmer<EmptyView>.center(shimmer, width: 160, elapsed: sweep / 2) ?? .nan) - 80) < 0.001)
        #expect(TextShimmer<EmptyView>.center(shimmer, width: 160, elapsed: sweep + 0.5) == nil)
        #expect(TextShimmer<EmptyView>.center(shimmer, width: 160, elapsed: sweep + 1) == -70)
    }

    @Test("The band's stops are spread evenly across its width")
    func shimmerStops() {
        let stops = TextShimmer<EmptyView>.stops(SlideStyle.shimmer)
        #expect(stops.count == 13)
        #expect(stops.first?.location == 0)
        #expect(stops.last?.location == 1)
        #expect(abs(stops[6].location - 0.5) < 0.0001)
    }

    @Test("The approach to the end takes 117 ms up to 42 points, longer as the square root beyond")
    func approach() {
        let feel = SlideStyle.feel
        #expect(abs(feel.approach(distance: 10) - 0.117) < 0.0001)
        #expect(abs(feel.approach(distance: 168) - 0.234) < 0.0001)
    }

    @Test("The pop runs the existing slider's table, peaks at 1.237 standing 3.33 past the end, and ends at rest")
    func pop() {
        let feel = SlideStyle.feel
        #expect(feel.popPeak == 1.237)
        #expect(feel.pop(at: 0, from: 1.059)?.scale == 1.059)
        let peak = feel.pop(at: 5, from: 1.059)
        #expect(peak?.scale == 1.237)
        #expect(abs((peak?.past ?? 0) - 3.33) < 0.0001)
        let under = feel.pop(at: 20, from: 1.059)
        #expect(under?.scale == 0.976)
        #expect(under?.past == 0)
        #expect(feel.pop(at: 27, from: 1.059) == nil)
    }
}
