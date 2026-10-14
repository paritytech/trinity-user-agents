import Foundation

/// Computes text differences (identify unchanged, added, and deleted portions) using the Longest Common Subsequence
/// (LCS) algorithm.

public final class TextDiffCalculator {
    public enum DiffPart: Equatable, Hashable {
        case unchanged(String)
        case added(String)
        case deleted(String)
    }

    public init() {}

    public func computeDiff(from oldText: String, to newText: String) -> [DiffPart] {
        let oldWords = tokenize(oldText)
        let newWords = tokenize(newText)

        let lcs = longestCommonSubsequence(oldWords, newWords)

        var result: [DiffPart] = []
        var oldIndex = 0
        var newIndex = 0
        var lcsIndex = 0

        while oldIndex < oldWords.count || newIndex < newWords.count {
            if lcsIndex < lcs.count,
               oldIndex < oldWords.count,
               newIndex < newWords.count,
               oldWords[oldIndex] == lcs[lcsIndex],
               newWords[newIndex] == lcs[lcsIndex] {
                result.append(.unchanged(oldWords[oldIndex]))
                oldIndex += 1
                newIndex += 1
                lcsIndex += 1
            } else if oldIndex < oldWords.count,
                      lcsIndex >= lcs.count || oldWords[oldIndex] != lcs[lcsIndex] {
                result.append(.deleted(oldWords[oldIndex]))
                oldIndex += 1
            } else if newIndex < newWords.count,
                      lcsIndex >= lcs.count || newWords[newIndex] != lcs[lcsIndex] {
                result.append(.added(newWords[newIndex]))
                newIndex += 1
            }
        }

        return mergeParts(result)
    }
}

// MARK: - Private

extension TextDiffCalculator {
    private func tokenize(_ text: String) -> [String] {
        var tokens: [String] = []
        var currentWord = ""

        for char in text {
            if char.isWhitespace || char.isPunctuation {
                if !currentWord.isEmpty {
                    tokens.append(currentWord)
                    currentWord = ""
                }
                tokens.append(String(char))
            } else {
                currentWord.append(char)
            }
        }

        if !currentWord.isEmpty {
            tokens.append(currentWord)
        }

        return tokens
    }

    private func longestCommonSubsequence(_ oldTokens: [String], _ newTokens: [String]) -> [String] {
        let oldCount = oldTokens.count
        let newCount = newTokens.count

        guard oldCount > 0, newCount > 0 else { return [] }

        var matrix = [[Int]](repeating: [Int](repeating: 0, count: newCount + 1), count: oldCount + 1)

        for row in 1 ... oldCount {
            for col in 1 ... newCount {
                if oldTokens[row - 1] == newTokens[col - 1] {
                    matrix[row][col] = matrix[row - 1][col - 1] + 1
                } else {
                    matrix[row][col] = max(matrix[row - 1][col], matrix[row][col - 1])
                }
            }
        }

        var lcs: [String] = []
        var row = oldCount
        var col = newCount
        while row > 0, col > 0 {
            if oldTokens[row - 1] == newTokens[col - 1] {
                lcs.append(oldTokens[row - 1])
                row -= 1
                col -= 1
            } else if matrix[row - 1][col] > matrix[row][col - 1] {
                row -= 1
            } else {
                col -= 1
            }
        }

        return lcs.reversed()
    }

    private func mergeParts(_ parts: [DiffPart]) -> [DiffPart] {
        var merged: [DiffPart] = []

        for part in parts {
            if let last = merged.last {
                switch (last, part) {
                case let (.unchanged(prev), .unchanged(curr)):
                    merged.removeLast()
                    merged.append(.unchanged(prev + curr))
                case let (.added(prev), .added(curr)):
                    merged.removeLast()
                    merged.append(.added(prev + curr))
                case let (.deleted(prev), .deleted(curr)):
                    merged.removeLast()
                    merged.append(.deleted(prev + curr))
                default:
                    merged.append(part)
                }
            } else {
                merged.append(part)
            }
        }

        return merged
    }
}
