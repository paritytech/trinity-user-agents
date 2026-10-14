import Foundation

enum IPV6Parser {
    static func parse(_ input: String) -> [UInt16]? {
        // Reject zone identifiers
        guard !input.contains("%") else { return nil }

        let parts = input.split(separator: ":", omittingEmptySubsequences: false)

        if input.contains("::") {
            // "::" compression handling
            guard parts.filter(\.isEmpty).count <= 2 else { return nil }

            let head = parts.prefix { !$0.isEmpty }
            let tail = parts.reversed().prefix { !$0.isEmpty }.reversed()

            let headValues = head.compactMap { UInt16($0, radix: 16) }
            let tailValues = tail.compactMap { UInt16($0, radix: 16) }

            guard headValues.count == head.count,
                  tailValues.count == tail.count,
                  headValues.count + tailValues.count <= 8
            else { return nil }

            let zeros = Array(repeating: UInt16(0), count: 8 - headValues.count - tailValues.count)
            return headValues + zeros + tailValues
        } else {
            // No compression
            guard parts.count == 8 else { return nil }

            let values = parts.compactMap { UInt16($0, radix: 16) }
            return values.count == 8 ? values : nil
        }
    }

    static func format(_ segments: [UInt16]) -> String {
        // Find longest zero run
        var bestStart: Int?
        var bestLen = 0
        var index = 0

        while index < 8 {
            if segments[index] == 0 {
                let start = index
                while index < 8, segments[index] == 0 {
                    index += 1
                }
                let len = index - start
                if len > bestLen {
                    bestLen = len
                    bestStart = start
                }
            } else {
                index += 1
            }
        }

        guard bestLen >= 2, let start = bestStart else {
            return segments.map { String($0, radix: 16) }.joined(separator: ":")
        }

        let head = segments[..<start].map { String($0, radix: 16) }
        let tail = segments[(start + bestLen)...].map { String($0, radix: 16) }

        var result = head.joined(separator: ":")
        result += "::"
        result += tail.joined(separator: ":")

        return result
    }
}
