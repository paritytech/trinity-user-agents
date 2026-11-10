import Foundation
import SubstrateSdk
import FoundationExt

struct ChainLiveness {
    let blockPeriod: Duration
    let slotCount: Int

    private let windowSeconds: Double

    private var firstRecordedAt: Date?
    private var samples: [(height: BlockNumber, date: Date)] = []

    init(blockPeriod: Duration) {
        self.blockPeriod = blockPeriod
        let blockSeconds = blockPeriod.timeInterval
        let calculatedWindow = max(30.0, blockSeconds * 10)

        windowSeconds = calculatedWindow
        slotCount = Int(calculatedWindow / blockSeconds)
    }

    /// Seeds history from a chain-time measurement so liveness is correct at once rather than
    /// after a full window. The anchor provider rejects a non-positive span; this repeats the
    /// check because mapping one to a full window would report a healthy chain on input already
    /// known to be inconsistent.
    mutating func apply(_ anchor: ChainLivenessAnchor, at date: Date) {
        let span = anchor.chainTimeSpanSeconds

        guard span > 0 else {
            return
        }

        let maxSlots = Double(slotCount)

        let effectiveSlots: UInt32
        if span <= windowSeconds {
            effectiveSlots = UInt32(slotCount)
        } else {
            let computed = floor(maxSlots * windowSeconds / span)
            effectiveSlots = UInt32(max(0, min(Double(slotCount), computed)))
        }

        clear()

        guard anchor.headHeight >= effectiveSlots else {
            record(height: 0, at: date.addingTimeInterval(-windowSeconds))
            record(height: anchor.headHeight, at: date)
            return
        }

        record(height: anchor.headHeight - effectiveSlots, at: date.addingTimeInterval(-windowSeconds))
        record(height: anchor.headHeight, at: date)
    }

    mutating func record(height: BlockNumber, at date: Date) {
        if firstRecordedAt == nil {
            firstRecordedAt = date
        }

        samples.append((height: height, date: date))
        dropExpiredSamples(before: date)
    }

    func liveness(at date: Date) -> Double? {
        guard
            let firstRecorded = firstRecordedAt,
            date.timeIntervalSince(firstRecorded) >= windowSeconds,
            let oldestRetained = samples.first,
            let newestSample = samples.last else {
            return nil
        }

        let windowStart = date.addingTimeInterval(-windowSeconds)
        let anchor = samples.last { $0.date <= windowStart } ?? oldestRetained
        // BlockNumber is unsigned: a reorg deeper than the window puts the head below the
        // anchor, and the subtraction would trap before any clamp could run.
        let heightDelta = newestSample.height >= anchor.height
            ? newestSample.height - anchor.height
            : 0

        return min(1.0, Double(heightDelta) / Double(slotCount))
    }

    mutating func clear() {
        samples.removeAll()
        firstRecordedAt = nil
    }

    /// Retains the newest sample at or before the window start: it is the anchor the block count is
    /// measured from, so dropping it would let a stalled chain read as live.
    private mutating func dropExpiredSamples(before date: Date) {
        let windowStart = date.addingTimeInterval(-windowSeconds)

        guard let anchorIndex = samples.lastIndex(where: { $0.date <= windowStart }) else {
            return
        }

        samples.removeFirst(anchorIndex)
    }
}
