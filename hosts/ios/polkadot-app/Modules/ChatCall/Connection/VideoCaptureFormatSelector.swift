import CoreMedia
import Foundation

struct VideoCaptureFormatCandidate {
    let id: Int
    let dimensions: CMVideoDimensions
    let frameRateRanges: [ClosedRange<Double>]
}

struct VideoCaptureFormatSelection {
    let candidate: VideoCaptureFormatCandidate
    let fps: Int
    let supportsTargetFps: Bool
    let fitsTargetDimensions: Bool
}

enum VideoCaptureFormatSelector {
    static func select(
        from candidates: [VideoCaptureFormatCandidate],
        preferences: VideoCapturePreferences
    ) -> VideoCaptureFormatSelection? {
        candidates
            .compactMap { makeSelection(from: $0, preferences: preferences) }
            .sorted { isWorse($0, than: $1, preferences: preferences) }
            .last
    }
}

private extension VideoCaptureFormatSelector {
    static func makeSelection(
        from candidate: VideoCaptureFormatCandidate,
        preferences: VideoCapturePreferences
    ) -> VideoCaptureFormatSelection? {
        guard let fps = preferredFps(for: candidate, preferences: preferences) else {
            return nil
        }

        let shortSide = min(candidate.dimensions.width, candidate.dimensions.height)
        let longSide = max(candidate.dimensions.width, candidate.dimensions.height)
        let targetShortSide = min(preferences.targetWidth, preferences.targetHeight)
        let targetLongSide = max(preferences.targetWidth, preferences.targetHeight)
        let fitsTargetDimensions = shortSide <= targetShortSide && longSide <= targetLongSide
        let supportsTargetFps = supportsTargetFps(candidate, preferences: preferences)

        return VideoCaptureFormatSelection(
            candidate: candidate,
            fps: fps,
            supportsTargetFps: supportsTargetFps,
            fitsTargetDimensions: fitsTargetDimensions
        )
    }

    static func preferredFps(
        for candidate: VideoCaptureFormatCandidate,
        preferences: VideoCapturePreferences
    ) -> Int? {
        if supportsTargetFps(candidate, preferences: preferences) {
            return preferences.targetFps
        }

        let targetFps = Double(preferences.targetFps)
        let lowerFps = candidate.frameRateRanges
            .map(\.upperBound)
            .filter { $0 < targetFps }
            .max()
        let higherFps = candidate.frameRateRanges
            .map(\.lowerBound)
            .filter { $0 > targetFps }
            .min()

        switch (lowerFps, higherFps) {
        case let (.some(lowerFps), .some(higherFps)):
            let lowerDistance = targetFps - lowerFps
            let higherDistance = higherFps - targetFps
            return Int(lowerDistance <= higherDistance ? lowerFps : higherFps)
        case let (.some(lowerFps), .none):
            return Int(lowerFps)
        case let (.none, .some(higherFps)):
            return Int(higherFps)
        case (.none, .none):
            return nil
        }
    }

    static func supportsTargetFps(
        _ candidate: VideoCaptureFormatCandidate,
        preferences: VideoCapturePreferences
    ) -> Bool {
        let targetFps = Double(preferences.targetFps)

        return candidate.frameRateRanges.contains {
            $0.lowerBound <= targetFps && targetFps <= $0.upperBound
        }
    }

    static func isWorse(
        _ lhs: VideoCaptureFormatSelection,
        than rhs: VideoCaptureFormatSelection,
        preferences: VideoCapturePreferences
    ) -> Bool {
        if lhs.fitsTargetDimensions != rhs.fitsTargetDimensions {
            return !lhs.fitsTargetDimensions
        }

        let lhsShortSide = min(lhs.candidate.dimensions.width, lhs.candidate.dimensions.height)
        let lhsLongSide = max(lhs.candidate.dimensions.width, lhs.candidate.dimensions.height)
        let rhsShortSide = min(rhs.candidate.dimensions.width, rhs.candidate.dimensions.height)
        let rhsLongSide = max(rhs.candidate.dimensions.width, rhs.candidate.dimensions.height)

        if lhsShortSide != rhsShortSide {
            return lhs.fitsTargetDimensions ? lhsShortSide < rhsShortSide : lhsShortSide > rhsShortSide
        }

        if lhsLongSide != rhsLongSide {
            return lhs.fitsTargetDimensions ? lhsLongSide < rhsLongSide : lhsLongSide > rhsLongSide
        }

        if lhs.supportsTargetFps != rhs.supportsTargetFps {
            return !lhs.supportsTargetFps
        }

        let lhsFpsDistance = abs(lhs.fps - preferences.targetFps)
        let rhsFpsDistance = abs(rhs.fps - preferences.targetFps)

        if lhsFpsDistance != rhsFpsDistance {
            return lhsFpsDistance > rhsFpsDistance
        }

        return lhs.fps < rhs.fps
    }
}
