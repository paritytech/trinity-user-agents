import Foundation
import QuartzCore

#if targetEnvironment(simulator)
    /// Measures the frames of the screen for the performance rows of the truapi E2E harness (P7.1, 04-TESTING §5).
    ///
    /// A display link calls the meter once for each frame of the screen. The meter keeps the time between two
    /// calls. When the main thread is busy for longer than one frame, the display link skips the frames in
    /// between, and the time between two calls is two frames or more. A frame time near `budget` is a smooth
    /// frame, and a frame time of 1.5 `budget` or more is a dropped frame.
    @MainActor
    final class TrUAPIE2EFrameMeter {
        private var link: CADisplayLink?
        private var last: CFTimeInterval?
        private var onFrame: (() -> Bool)?
        private var completion: ((TrUAPIE2EFrameMeter) -> Void)?
        private let startedAt = ContinuousClock.now

        /// The time between frames, in milliseconds.
        private(set) var frameTimes: [Double] = []
        /// The time of one frame of the screen, in milliseconds.
        private(set) var budget: Double = 1_000 / 60

        /// Starts the display link. `onFrame` runs once for each frame and gives false to stop the meter.
        /// `completion` runs once after the last frame.
        func start(onFrame: @escaping () -> Bool, completion: @escaping (TrUAPIE2EFrameMeter) -> Void) {
            self.onFrame = onFrame
            self.completion = completion
            let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
            link.add(to: .main, forMode: .common)
            self.link = link
        }

        /// Measures for `duration` and does nothing on each frame.
        func measure(for duration: Duration, completion: @escaping (TrUAPIE2EFrameMeter) -> Void) {
            let end = ContinuousClock.now + duration
            start(onFrame: { ContinuousClock.now < end }, completion: completion)
        }

        /// One line for the console: the frame count, the percentiles and the dropped frames.
        var summary: String {
            let sorted = frameTimes.sorted()
            func percentile(_ fraction: Double) -> Double {
                guard !sorted.isEmpty else { return 0 }
                return sorted[min(sorted.count - 1, Int(Double(sorted.count - 1) * fraction))]
            }
            let dropped = frameTimes.filter { $0 >= budget * 1.5 }.count
            let elapsed = ContinuousClock.now - startedAt
            let seconds = Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18
            return [
                "frames=\(frameTimes.count)",
                "budget=\(Self.format(budget))",
                "p50=\(Self.format(percentile(0.5)))",
                "p95=\(Self.format(percentile(0.95)))",
                "p99=\(Self.format(percentile(0.99)))",
                "max=\(Self.format(sorted.last ?? 0))",
                "dropped=\(dropped)",
                "seconds=\(Self.format(seconds))",
                "footprintMB=\(Self.format(Self.footprintMegabytes()))"
            ].joined(separator: " ")
        }

        /// The memory that the system counts for the app, in megabytes.
        static func footprintMegabytes() -> Double {
            var info = task_vm_info_data_t()
            var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
            let result = withUnsafeMutablePointer(to: &info) { pointer in
                pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                    task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
                }
            }
            guard result == KERN_SUCCESS else { return 0 }
            return Double(info.phys_footprint) / 1_048_576
        }

        @objc private func tick(_ link: CADisplayLink) {
            if let last {
                frameTimes.append((link.timestamp - last) * 1_000)
            }
            last = link.timestamp
            budget = (link.targetTimestamp - link.timestamp) * 1_000

            guard onFrame?() == true else {
                link.invalidate()
                self.link = nil
                onFrame = nil
                completion?(self)
                completion = nil
                return
            }
        }

        private static func format(_ value: Double) -> String {
            String(format: "%.1f", value)
        }
    }
#endif
