import Foundation

public protocol CountdownDateFormatting {
    func formatWithSinglePart(to date: Date) -> String
    func formatWithMultipleParts(to date: Date) -> String
    func formatCompact(to date: Date) -> String
}
