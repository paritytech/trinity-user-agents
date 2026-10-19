#if SENTRY_ENABLED
    import Foundation
    import Sentry
    import UIKit

    /// Sends the device readings the OS gives without a permission to Sentry as gauges, once a minute.
    @MainActor
    final class SentryDeviceReadingsReporter {
        private let device = UIDevice.current
        private let processInfo = ProcessInfo.processInfo
        private let host = mach_host_self()
        private var previousCPUSample: CPUSample?

        func start() {
            device.isBatteryMonitoringEnabled = true

            Task {
                while !Task.isCancelled {
                    report()
                    try? await Task.sleep(for: .seconds(60))
                }
            }
        }
    }

    private extension SentryDeviceReadingsReporter {
        struct CPUSample {
            let processSeconds: Double
            let wallClock: Date

            static func now() -> CPUSample {
                var usage = rusage()
                getrusage(RUSAGE_SELF, &usage)
                return CPUSample(processSeconds: usage.ru_utime.seconds + usage.ru_stime.seconds, wallClock: Date())
            }

            func usagePercent(since previous: CPUSample, coreCount: Int) -> Double {
                let wallSeconds = wallClock.timeIntervalSince(previous.wallClock) * Double(coreCount)
                return (processSeconds - previous.processSeconds) * 100 / wallSeconds
            }
        }

        func report() {
            let coreCount = processInfo.activeProcessorCount
            let cpuSample = CPUSample.now()
            let processCPUUsage = previousCPUSample.map { cpuSample.usagePercent(since: $0, coreCount: coreCount) }
            let storage = try? URL(fileURLWithPath: NSHomeDirectory()).resourceValues(
                forKeys: [.volumeAvailableCapacityForImportantUsageKey, .volumeTotalCapacityKey]
            )
            let freeStorage = storage?.volumeAvailableCapacityForImportantUsage.map(Double.init)
            let totalStorage = storage?.volumeTotalCapacity.map(Double.init)

            gauge("device.cpu.process_usage", processCPUUsage, unit: .percent)
            gauge("device.cpu.cores", Double(coreCount))
            gauge("device.memory.total", Double(processInfo.physicalMemory), unit: .byte)
            gauge("device.memory.available", availablePhoneMemory(), unit: .byte)
            gauge("device.memory.process", processMemoryFootprint(), unit: .byte)
            gauge("device.storage.free", freeStorage, unit: .byte)
            gauge("device.storage.total", totalStorage, unit: .byte)
            gauge("device.thermal.state", Double(processInfo.thermalState.rawValue))
            gauge("device.battery.level", batteryPercent(), unit: .percent)
            gauge("device.battery.charging", batteryCharging())

            previousCPUSample = cpuSample
        }

        func gauge(_ key: String, _ value: Double?, unit: SentryUnit? = nil) {
            guard let value else { return }

            SentrySDK.metrics.gauge(key: key, value: value, unit: unit)
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

        func batteryPercent() -> Double? {
            device.batteryLevel < 0 ? nil : Double(device.batteryLevel) * 100
        }

        func batteryCharging() -> Double? {
            switch device.batteryState {
            case .charging,
                 .full:
                1
            case .unplugged:
                0
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
#endif
