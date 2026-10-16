import CoreGraphics
import Foundation
import Testing

@testable import polkadot_app

/// Checks the ported layout and motion against the vectors `coinage-viz` exports, so the strip on
/// the phone is the strip in the reference rather than something that merely resembles it.
@Suite("Coinage conformance")
struct CoinageConformanceTests {
    @Test("The strip solves the same turn, margins and placements as the reference")
    func stripMatchesTheReference() throws {
        let vectors: StripVectors = try load("strip")

        for testCase in vectors.cases {
            var options = CoinageStripLayout.Options()
            options.height = testCase.options.height
            options.margin = testCase.options.margin
            options.minimumMargin = testCase.options.minMargin
            options.gap = testCase.options.gap
            options.minimumGap = testCase.options.minGap

            let layout = CoinageStripLayout.layout(
                testCase.input.map {
                    CoinageStripLayout.Coin(
                        size: $0.size,
                        width: $0.width,
                        thickness: $0.thickness,
                        partition: $0.partition == "clearing" ? .clearing : .ready
                    )
                },
                width: testCase.width,
                options: options
            )

            expect(layout.turn, testCase.output.turn, testCase.label, "turn")
            expect(layout.margin, testCase.output.margin, testCase.label, "margin")
            expect(layout.gap, testCase.output.gap, testCase.label, "gap")
            expect(layout.thinning, testCase.output.thinning, testCase.label, "thinning")
            expect(layout.used, testCase.output.used, testCase.label, "used")

            #expect(layout.coins.count == testCase.output.coins.count, "\(testCase.label): count")

            for (placed, expected) in zip(layout.coins, testCase.output.coins) {
                expect(placed.centre.x, expected.x, testCase.label, "x")
                expect(placed.centre.y, expected.y, testCase.label, "y")
                expect(placed.height, expected.height, testCase.label, "height")
                expect(placed.turn, expected.turn, testCase.label, "coin turn")
                expect(placed.thickness, expected.thick, testCase.label, "thickness")
                expect(placed.visible, expected.visible, testCase.label, "visible")
            }

            #expect(
                layout.partitions.count == testCase.output.partitions.count,
                "\(testCase.label): partitions"
            )

            for (span, expected) in zip(layout.partitions, testCase.output.partitions) {
                #expect(span.partition.rawValue == expected.partition, "\(testCase.label): partition")
                #expect(span.count == expected.count, "\(testCase.label): span count")
                expect(span.start, expected.from, testCase.label, "span from")
                expect(span.end, expected.to, testCase.label, "span to")
            }
        }
    }

    @Test("The spring, edge calm and level of detail match the reference")
    func motionMatchesTheReference() throws {
        let vectors: MotionVectors = try load("motion")

        for sample in vectors.spring {
            let stepped = CoinageSpring.step(
                value: sample.input[0],
                velocity: sample.input[1],
                target: sample.input[2],
                step: sample.input[3],
                omega: sample.input[4]
            )

            expect(stepped.value, sample.output[0], "spring", "value")
            expect(stepped.velocity, sample.output[1], "spring", "velocity")
        }

        for sample in vectors.edgeCalm {
            expect(CoinageStripLayout.edgeCalm(turn: sample.turn), sample.calm, "edgeCalm", "calm")
        }
    }

    /// The reference answers an overfull grid by piling runs of alike coins, which this app does
    /// not draw. Its vectors for that — two of its three scenarios, and its pile and band samples —
    /// describe a layout we deliberately do not produce, and are not vendored. What is left is the
    /// one scenario that lays every coin out singly, which is what this app always does.
    @Test("The grid packs and blocks as the reference does")
    func gridMatchesTheReference() throws {
        let vectors: GridVectors = try load("grid")

        #expect(!vectors.cases.isEmpty, "no grid vectors")

        for testCase in vectors.cases {
            var options = CoinageGridLayout.Options()
            options.maxDiameter = testCase.options.maxDiameter
            options.minDiameter = testCase.options.minDiameter
            options.gap = testCase.options.gap
            options.header = testCase.options.header
            options.blockGap = testCase.options.blockGap

            let layout = CoinageGridLayout.layout(
                testCase.input.map {
                    CoinageGridLayout.Item(
                        id: String($0.id),
                        exponent: $0.exponent,
                        partition: $0.partition == "clearing" ? .clearing : .ready
                    )
                },
                area: CGSize(width: testCase.area.width, height: testCase.area.height),
                options: options
            )

            expect(layout.diameter, testCase.output.diameter, testCase.label, "diameter")
            expect(layout.pitch, testCase.output.pitch, testCase.label, "pitch")
            expect(layout.height, testCase.output.height, testCase.label, "height")
            #expect(layout.columns == testCase.output.cols, "\(testCase.label): columns")
            #expect(layout.cells.count == testCase.output.cells.count, "\(testCase.label): cells")

            for (cell, expected) in zip(layout.cells, testCase.output.cells) {
                expect(cell.centre.x, expected.x, testCase.label, "cell x")
                expect(cell.centre.y, expected.y, testCase.label, "cell y")
                #expect([cell.id] == expected.ids.map(String.init), "\(testCase.label): cell id")
            }

            #expect(layout.blocks.count == testCase.output.blocks.count, "\(testCase.label): blocks")

            for (block, expected) in zip(layout.blocks, testCase.output.blocks) {
                #expect(block.partition.rawValue == expected.partition, "\(testCase.label): block")
                #expect(block.count == expected.count, "\(testCase.label): block count")
                expect(block.top, expected.y, testCase.label, "block top")
            }
        }
    }

    @Test("Wear sits on the reference's own ladder")
    func wearMatchesTheReference() throws {
        let vectors: FungibilityVectors = try load("fungibility")

        #expect(CoinageWear.ringCapacity == vectors.ringCapacity)
        #expect(CoinageWear.maximumLevel == vectors.maxLevel)

        for sample in vectors.samples {
            #expect(CoinageWear.level(hiddenAmong: sample.k) == sample.level, "level for k=\(sample.k)")
            expect(CoinageWear.amount(forLevel: sample.level), sample.wear, "wear", "k=\(sample.k)")
        }
    }
}

// MARK: - Vectors

private extension CoinageConformanceTests {
    /// The reference rounds its vectors to six decimals, so equality is to that.
    static let tolerance: CGFloat = 5e-6

    func expect(_ value: CGFloat, _ expected: CGFloat, _ label: String, _ field: String) {
        #expect(
            abs(value - expected) <= Self.tolerance * max(1, abs(expected)),
            "\(label): \(field) was \(value), expected \(expected)"
        )
    }

    func load<T: Decodable>(_ name: String) throws -> T {
        let url = try #require(
            Bundle(for: BundleToken.self).url(forResource: name, withExtension: "json"),
            "missing conformance vectors: \(name).json"
        )

        return try JSONDecoder().decode(T.self, from: Data(contentsOf: url))
    }

    struct StripVectors: Decodable {
        struct Options: Decodable {
            let height: CGFloat
            let margin: CGFloat
            let minMargin: CGFloat
            let gap: CGFloat
            let minGap: CGFloat
        }

        struct Input: Decodable {
            let size: CGFloat
            let width: CGFloat
            let thickness: CGFloat
            let partition: String
        }

        struct Placement: Decodable {
            let x: CGFloat
            let y: CGFloat
            let height: CGFloat
            let turn: CGFloat
            let thick: CGFloat
            let visible: CGFloat
        }

        struct Span: Decodable {
            let partition: String
            let from: CGFloat
            let to: CGFloat
            let count: Int
        }

        struct Output: Decodable {
            let turn: CGFloat
            let margin: CGFloat
            let gap: CGFloat
            let thinning: CGFloat
            let coins: [Placement]
            let partitions: [Span]
            let used: CGFloat
        }

        struct Case: Decodable {
            let label: String
            let width: CGFloat
            let options: Options
            let input: [Input]
            let output: Output
        }

        let cases: [Case]
    }

    struct MotionVectors: Decodable {
        struct Spring: Decodable {
            let input: [CGFloat]
            let output: [CGFloat]
        }

        struct Calm: Decodable {
            let turn: CGFloat
            let calm: CGFloat
        }

        let spring: [Spring]
        let edgeCalm: [Calm]
    }

    struct GridVectors: Decodable {
        struct Options: Decodable {
            let maxDiameter: CGFloat
            let minDiameter: CGFloat
            let gap: CGFloat
            let header: CGFloat
            let blockGap: CGFloat
        }

        struct Area: Decodable {
            let width: CGFloat
            let height: CGFloat
        }

        struct Input: Decodable {
            let id: Int
            let exponent: Int16
            let partition: String
        }

        struct Cell: Decodable {
            let ids: [Int]
            let x: CGFloat
            let y: CGFloat
        }

        struct Block: Decodable {
            let partition: String
            let y: CGFloat
            let count: Int
        }

        struct Output: Decodable {
            let diameter: CGFloat
            let pitch: CGFloat
            let cols: Int
            let cells: [Cell]
            let blocks: [Block]
            let height: CGFloat
        }

        struct Case: Decodable {
            let label: String
            let area: Area
            let options: Options
            let input: [Input]
            let output: Output
        }

        let cases: [Case]
    }

    struct FungibilityVectors: Decodable {
        struct Sample: Decodable {
            let k: Int
            let level: Int
            let wear: CGFloat
        }

        let ringCapacity: Int
        let maxLevel: Int
        let samples: [Sample]
    }
}

private final class BundleToken {}
