@testable import polkadot_app
import CoreMedia
import Testing

enum VideoCaptureFormatSelectorTests {
    private static let gamePreferences = VideoCapturePreferences(maxWidth: 854, maxHeight: 480, maxFps: 24)

    @Test("Selects target FPS when supported by frame rate range")
    static func selectsTargetFpsWhenSupportedByRange() throws {
        let candidate = makeCandidate(
            id: 0,
            width: 640,
            height: 480,
            frameRateRanges: [1 ... 30]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [candidate],
            preferences: gamePreferences
        ))

        #expect(selection.candidate.id == 0)
        #expect(selection.fps == 24)
        #expect(selection.supportsTargetFps)
        #expect(selection.fitsTargetDimensions)
    }

    @Test("Prioritizes matching resolution over exact FPS")
    static func prioritizesMatchingResolutionOverExactFps() throws {
        let matchingResolutionLowerFps = makeCandidate(
            id: 0,
            width: 640,
            height: 480,
            frameRateRanges: [1 ... 16]
        )
        let lowerResolutionExactFps = makeCandidate(
            id: 1,
            width: 320,
            height: 240,
            frameRateRanges: [1 ... 30]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [matchingResolutionLowerFps, lowerResolutionExactFps],
            preferences: gamePreferences
        ))

        #expect(selection.candidate.id == 0)
        #expect(selection.fps == 16)
        #expect(!selection.supportsTargetFps)
    }

    @Test("Selects largest fitting resolution before comparing FPS")
    static func selectsLargestFittingResolutionBeforeComparingFps() throws {
        let smallerExactFps = makeCandidate(
            id: 0,
            width: 320,
            height: 240,
            frameRateRanges: [1 ... 30]
        )
        let largerExactFps = makeCandidate(
            id: 1,
            width: 640,
            height: 480,
            frameRateRanges: [1 ... 30]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [smallerExactFps, largerExactFps],
            preferences: gamePreferences
        ))

        #expect(selection.candidate.id == 1)
        #expect(selection.fps == 24)
    }

    @Test("Matches dimensions independently of target orientation")
    static func matchesDimensionsIndependentlyOfTargetOrientation() throws {
        let portraitPreferences = VideoCapturePreferences(maxWidth: 480, maxHeight: 854, maxFps: 24)
        let portraitCandidate = makeCandidate(
            id: 0,
            width: 480,
            height: 854,
            frameRateRanges: [1 ... 30]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [portraitCandidate],
            preferences: portraitPreferences
        ))

        #expect(selection.candidate.id == 0)
        #expect(selection.fitsTargetDimensions)
    }

    @Test("Falls back to smallest resolution above target")
    static func fallsBackToSmallestResolutionAboveTarget() throws {
        let smallerFallback = makeCandidate(
            id: 0,
            width: 960,
            height: 540,
            frameRateRanges: [1 ... 30]
        )
        let largerFallback = makeCandidate(
            id: 1,
            width: 1_280,
            height: 720,
            frameRateRanges: [1 ... 30]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [largerFallback, smallerFallback],
            preferences: gamePreferences
        ))

        #expect(selection.candidate.id == 0)
        #expect(selection.fps == 24)
        #expect(!selection.fitsTargetDimensions)
    }

    @Test("Selects closest lower FPS when target FPS is unsupported")
    static func selectsClosestLowerFpsWhenTargetFpsIsUnsupported() throws {
        let candidate = makeCandidate(
            id: 0,
            width: 640,
            height: 480,
            frameRateRanges: [1 ... 16]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [candidate],
            preferences: gamePreferences
        ))

        #expect(selection.fps == 16)
        #expect(!selection.supportsTargetFps)
    }

    @Test("Selects closest higher FPS when it is nearer than lower fallback")
    static func selectsClosestHigherFpsWhenNearerThanLowerFallback() throws {
        let candidate = makeCandidate(
            id: 0,
            width: 640,
            height: 480,
            frameRateRanges: [1 ... 16, 30 ... 60]
        )

        let selection = try #require(VideoCaptureFormatSelector.select(
            from: [candidate],
            preferences: gamePreferences
        ))

        #expect(selection.fps == 30)
        #expect(!selection.supportsTargetFps)
    }
}

private extension VideoCaptureFormatSelectorTests {
    static func makeCandidate(
        id: Int,
        width: Int32,
        height: Int32,
        frameRateRanges: [ClosedRange<Double>]
    ) -> VideoCaptureFormatCandidate {
        VideoCaptureFormatCandidate(
            id: id,
            dimensions: CMVideoDimensions(width: width, height: height),
            frameRateRanges: frameRateRanges
        )
    }
}
