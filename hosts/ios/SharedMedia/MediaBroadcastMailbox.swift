import Foundation
import CoreVideo
import CoreMedia
import Darwin

/// One bounded latest-frame slot, accessible only to the signed host and its
/// broadcast extension. No socket, URL scheme, or product-readable file handle.
final class MediaBroadcastMailbox {
    enum Failure: Error { case unavailable }
    static let capacity = 4096 * 4096 * 4 + 4096
    private let descriptor: Int32
    private let memory: UnsafeMutableRawPointer
    private var pool: CVPixelBufferPool?
    private var poolSize = CGSize.zero
    private var poolFormat: OSType = 0

    static var group: String? { Bundle.main.object(forInfoDictionaryKey: "MediaAppGroup") as? String }

    init(create: Bool) throws {
        guard let group = Self.group,
              let root = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group) else {
            throw Failure.unavailable
        }
        let path = root.appendingPathComponent("media-broadcast.frame").path
        descriptor = open(path, O_RDWR | (create ? O_CREAT : 0), S_IRUSR | S_IWUSR)
        guard descriptor >= 0 else { throw Failure.unavailable }
        if create && ftruncate(descriptor, off_t(Self.capacity)) != 0 {
            close(descriptor)
            throw Failure.unavailable
        }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_size == Self.capacity,
              let pointer = mmap(nil, Self.capacity, PROT_READ | PROT_WRITE, MAP_SHARED, descriptor, 0),
              pointer != MAP_FAILED else {
            close(descriptor)
            throw Failure.unavailable
        }
        memory = pointer
        if create { try locked { clearFrame(); memset(memory, 0, 4096) } }
    }

    deinit { munmap(memory, Self.capacity); close(descriptor) }

    private func locked<T>(_ body: () throws -> T) throws -> T {
        guard flock(descriptor, LOCK_EX) == 0 else { throw Failure.unavailable }
        defer { flock(descriptor, LOCK_UN) }
        return try body()
    }

    private func get(_ index: Int) -> Int64 { memory.load(fromByteOffset: index * 8, as: Int64.self) }
    private func set(_ index: Int, _ value: Int64) { memory.storeBytes(of: value, toByteOffset: index * 8, as: Int64.self) }

    private func clearFrame() {
        let count = min(max(0, Int(get(14))), Self.capacity - 4096)
        memset(memory.advanced(by: 4096), 0, count)
        set(14, 0)
    }

    /// New random generation on every trusted picker request. Late extensions
    /// cannot publish into a replacement request, even after cancellation.
    func prepare(generation: Int64) throws {
        try locked { set(0, generation); set(1, 1); set(2, 0); set(9, Int64(Date().timeIntervalSince1970)) }
    }

    func begin() throws -> Int64 {
        try locked {
            guard get(1) == 1, Date().timeIntervalSince1970 - Double(get(9)) < 120 else { throw Failure.unavailable }
            set(1, 2); set(9, Int64(Date().timeIntervalSince1970))
            return get(0)
        }
    }

    func status(generation: Int64) throws -> Int64 {
        try locked {
            guard get(0) == generation else { return 0 }
            if get(1) == 2 && Date().timeIntervalSince1970 - Double(get(9)) >= 5 { set(1, 0); clearFrame() }
            return get(1)
        }
    }

    func stop(generation: Int64) {
        try? locked {
            guard get(0) == generation else { return }
            set(1, 0); set(2, 0)
            clearFrame()
        }
    }

    func heartbeat(generation: Int64) throws {
        try locked {
            guard get(0) == generation, get(1) != 0 else { throw Failure.unavailable }
            set(9, Int64(Date().timeIntervalSince1970))
        }
    }

    func publish(_ buffer: CVPixelBuffer, time: CMTime, rotation: Int64, generation: Int64) throws {
        try locked {
            guard get(0) == generation, get(1) == 2,
                  Date().timeIntervalSince1970 - Double(get(9)) < 5 else { throw Failure.unavailable }
            let seconds = CMTimeGetSeconds(time)
            guard seconds.isFinite, seconds >= 0, seconds < Double(Int64.max) / 1_000_000_000 else { throw Failure.unavailable }
            CVPixelBufferLockBaseAddress(buffer, .readOnly)
            defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
            let width = CVPixelBufferGetWidth(buffer), height = CVPixelBufferGetHeight(buffer)
            guard width > 0, height > 0, width <= 4096, height <= 4096 else { throw Failure.unavailable }
            let planar = CVPixelBufferIsPlanar(buffer)
            let count = planar ? CVPixelBufferGetPlaneCount(buffer) : 1
            guard count <= 2 else { throw Failure.unavailable }
            var offset = 4096
            for plane in 0..<count {
                let stride = planar ? CVPixelBufferGetBytesPerRowOfPlane(buffer, plane) : CVPixelBufferGetBytesPerRow(buffer)
                let rows = planar ? CVPixelBufferGetHeightOfPlane(buffer, plane) : height
                let base = planar ? CVPixelBufferGetBaseAddressOfPlane(buffer, plane) : CVPixelBufferGetBaseAddress(buffer)
                guard let base, offset + stride * rows <= Self.capacity else { throw Failure.unavailable }
                memcpy(memory.advanced(by: offset), base, stride * rows)
                set(10 + plane * 2, Int64(stride)); set(11 + plane * 2, Int64(rows))
                offset += stride * rows
            }
            set(3, Int64(width)); set(4, Int64(height)); set(5, Int64(CVPixelBufferGetPixelFormatType(buffer)))
            set(6, Int64(seconds * 1_000_000_000)); set(7, rotation); set(8, Int64(count))
            set(14, max(get(14), Int64(offset - 4096)))
            set(2, get(2) &+ 1)
        }
    }

    func frame(after sequence: Int64, generation: Int64) throws -> (CVPixelBuffer, Int64, Int64, Int64)? {
        try locked {
            guard get(0) == generation, get(1) == 2 else { throw Failure.unavailable }
            guard get(2) != 0, get(2) != sequence else { return nil }
            let width = Int(get(3)), height = Int(get(4)), format = OSType(get(5))
            let size = CGSize(width: width, height: height)
            if pool == nil || size != poolSize || format != poolFormat {
                pool = nil
                let attributes: [String: Any] = [kCVPixelBufferWidthKey as String: width,
                    kCVPixelBufferHeightKey as String: height, kCVPixelBufferPixelFormatTypeKey as String: format,
                    kCVPixelBufferIOSurfacePropertiesKey as String: [:]]
                guard CVPixelBufferPoolCreate(nil, nil, attributes as CFDictionary, &pool) == kCVReturnSuccess else {
                    throw Failure.unavailable
                }
                poolSize = size; poolFormat = format
            }
            guard let pool else { throw Failure.unavailable }
            var output: CVPixelBuffer?
            guard CVPixelBufferPoolCreatePixelBuffer(nil, pool, &output) == kCVReturnSuccess,
                  let output else { throw Failure.unavailable }
            CVPixelBufferLockBaseAddress(output, [])
            defer { CVPixelBufferUnlockBaseAddress(output, []) }
            let planar = CVPixelBufferIsPlanar(output)
            var offset = 4096
            for plane in 0..<Int(get(8)) {
                let inputStride = Int(get(10 + plane * 2)), rows = Int(get(11 + plane * 2))
                let stride = planar ? CVPixelBufferGetBytesPerRowOfPlane(output, plane) : CVPixelBufferGetBytesPerRow(output)
                let base = planar ? CVPixelBufferGetBaseAddressOfPlane(output, plane) : CVPixelBufferGetBaseAddress(output)
                guard let base else { throw Failure.unavailable }
                for row in 0..<rows {
                    memcpy(base.advanced(by: row * stride), memory.advanced(by: offset + row * inputStride), min(stride, inputStride))
                }
                offset += inputStride * rows
            }
            return (output, get(2), get(6), get(7))
        }
    }
}
