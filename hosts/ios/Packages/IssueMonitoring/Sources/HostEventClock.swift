import Foundation

/// The clock `event.monotonic_ms` is read from.
public enum HostEventClock {
    /// Whole milliseconds on `CLOCK_MONOTONIC`, which keeps counting while the device
    /// sleeps, as Android's `elapsedRealtime` does.
    public static func monotonicMilliseconds() -> Int {
        Int(clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000)
    }
}
