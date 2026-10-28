import Foundation
import UIKit
#if SENTRY_ENABLED
    import os
    import Sentry
#endif

/// Takes device readings once a minute while the app is in the foreground.
public protocol DeviceReadingsMonitoring {
    func start()
}

struct DeviceReadings {
    let processCPUUsagePercent: Double?
    let coreCount: Int
    let memoryTotalBytes: Double
    let memoryAvailableBytes: Double?
    let processMemoryBytes: Double?
    let storageFreeBytes: Double?
    let storageTotalBytes: Double?
    let thermalState: Int
    let batteryLevelPercent: Double?
    let charging: Bool?

    var logLine: String {
        [
            processCPUUsagePercent.map { String(format: "cpu=%.1f%%", $0) },
            "cores=\(coreCount)",
            memoryAvailableBytes.map { "memory=\(megabytes($0))/\(megabytes(memoryTotalBytes))MB" },
            processMemoryBytes.map { "process=\(megabytes($0))MB" },
            storageFreeBytes.flatMap { free in
                storageTotalBytes.map { "storage=\(megabytes(free))/\(megabytes($0))MB" }
            },
            "thermal=\(thermalState)",
            batteryLevelPercent.map { String(format: "battery=%.0f%%", $0) },
            charging.map { "charging=\($0)" }
        ]
        .compactMap { $0 }
        .joined(separator: " ")
    }

    private func megabytes(_ bytes: Double) -> String {
        String(Int(bytes / 1_000_000))
    }
}

final class DeviceReadingsMonitor: DeviceReadingsMonitoring {
    private let report: (DeviceReadings) -> Void
    private let processInfo = ProcessInfo.processInfo
    private let host = mach_host_self()
    private var loop: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []

    init(report: @escaping (DeviceReadings) -> Void) {
        self.report = report
    }

    func start() {
        let center = NotificationCenter.default
        observers = [
            center.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.resume()
            },
            center.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.pause()
            }
        ]

        if UIApplication.shared.applicationState != .background {
            resume()
        }
    }
}

private extension DeviceReadingsMonitor {
    struct CPUSample {
        let processSeconds: Double
        let instant: ContinuousClock.Instant

        static func now() -> CPUSample {
            var usage = rusage()
            getrusage(RUSAGE_SELF, &usage)
            return CPUSample(
                processSeconds: usage.ru_utime.seconds + usage.ru_stime.seconds,
                instant: ContinuousClock.now
            )
        }

        func usagePercent(since previous: CPUSample, coreCount: Int) -> Double {
            let wallSeconds = previous.instant.duration(to: instant) / .seconds(1) * Double(coreCount)
            return (processSeconds - previous.processSeconds) * 100 / wallSeconds
        }
    }

    func resume() {
        guard loop == nil else { return }

        loop = Task {
            await enableBatteryMonitoring()
            var previousCPUSample: CPUSample?
            while !Task.isCancelled {
                let cpuSample = CPUSample.now()
                await report(read(processCPUUsagePercent: previousCPUSample.map {
                    cpuSample.usagePercent(since: $0, coreCount: processInfo.activeProcessorCount)
                }))
                previousCPUSample = cpuSample
                try? await Task.sleep(for: .seconds(60))
            }
        }
    }

    func pause() {
        loop?.cancel()
        loop = nil
    }

    func read(processCPUUsagePercent: Double?) async -> DeviceReadings {
        let storage = try? URL(fileURLWithPath: NSHomeDirectory()).resourceValues(
            forKeys: [.volumeAvailableCapacityForImportantUsageKey, .volumeTotalCapacityKey]
        )

        return await DeviceReadings(
            processCPUUsagePercent: processCPUUsagePercent,
            coreCount: processInfo.activeProcessorCount,
            memoryTotalBytes: Double(processInfo.physicalMemory),
            memoryAvailableBytes: availablePhoneMemory(),
            processMemoryBytes: processMemoryFootprint(),
            storageFreeBytes: storage?.volumeAvailableCapacityForImportantUsage.map(Double.init),
            storageTotalBytes: storage?.volumeTotalCapacity.map(Double.init),
            thermalState: processInfo.thermalState.rawValue,
            batteryLevelPercent: batteryPercent(),
            charging: batteryCharging()
        )
    }

    func availablePhoneMemory() -> Double? {
        var pageSize: vm_size_t = 0
        guard
            host_page_size(host, &pageSize) == KERN_SUCCESS,
            let statistics = machInfo(vm_statistics64(), fetch: {
                host_statistics64(host, HOST_VM_INFO64, $0, $1)
            })
        else {
            return nil
        }

        let pageCount = UInt64(statistics.free_count) + UInt64(statistics.inactive_count)
        return Double(pageCount * UInt64(pageSize))
    }

    func processMemoryFootprint() -> Double? {
        machInfo(task_vm_info_data_t(), fetch: {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, $1)
        }).map { Double($0.phys_footprint) }
    }

    func machInfo<Info>(
        _ empty: Info,
        fetch: (UnsafeMutablePointer<integer_t>, UnsafeMutablePointer<mach_msg_type_number_t>) -> kern_return_t
    ) -> Info? {
        var info = empty
        var count = mach_msg_type_number_t(MemoryLayout<Info>.size / MemoryLayout<integer_t>.size)
        let result = withUnsafeMutablePointer(to: &info) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) { fetch($0, &count) }
        }
        return result == KERN_SUCCESS ? info : nil
    }

    @MainActor
    func enableBatteryMonitoring() {
        UIDevice.current.isBatteryMonitoringEnabled = true
    }

    @MainActor
    func batteryPercent() -> Double? {
        let level = UIDevice.current.batteryLevel
        return level < 0 ? nil : Double(level) * 100
    }

    @MainActor
    func batteryCharging() -> Bool? {
        switch UIDevice.current.batteryState {
        case .charging,
             .full:
            true
        case .unplugged:
            false
        case .unknown:
            nil
        @unknown default:
            nil
        }
    }
}

private extension timeval {
    var seconds: Double {
        Double(tv_sec) + Double(tv_usec) / 1_000_000
    }
}

#if SENTRY_ENABLED
    extension DeviceReadings {
        private static let launchTotalsSent = OSAllocatedUnfairLock(initialState: false)

        func sendSentryGauges() {
            guard SentrySDK.isEnabled else { return }

            if let storageTotalBytes {
                Self.launchTotalsSent.withLock { sent in
                    guard !sent else { return }

                    gauge("device.memory.total", memoryTotalBytes, unit: .byte)
                    gauge("device.storage.total", storageTotalBytes, unit: .byte)
                    sent = true
                }
            }
            gauge("device.cpu.process_usage", processCPUUsagePercent, unit: .percent)
            gauge("device.cpu.cores", Double(coreCount))
            gauge("device.memory.available", memoryAvailableBytes, unit: .byte)
            gauge("device.memory.process", processMemoryBytes, unit: .byte)
            gauge("device.storage.free", storageFreeBytes, unit: .byte)
            gauge("device.thermal.state.ios", Double(thermalState))
            gauge("device.battery.level", batteryLevelPercent, unit: .percent)
            gauge("device.battery.charging", charging.map { $0 ? 1 : 0 })
        }

        private func gauge(_ key: String, _ value: Double?, unit: SentryUnit? = nil) {
            guard let value else { return }

            SentrySDK.metrics.gauge(key: key, value: value, unit: unit)
        }
    }
#endif
