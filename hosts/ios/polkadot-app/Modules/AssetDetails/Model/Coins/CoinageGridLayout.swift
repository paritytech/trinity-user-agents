import CoreGraphics
import Foundation

/// The coin grid: coins face on, packed in offset rows like a honeycomb, in the same order as the
/// strip, one block per partition under its header.
///
/// Ported from `src/layout/hexgrid.js` and checked against the reference's own conformance
/// vectors. The pitch is as tight as fits; once even the smallest pitch will not hold every coin,
/// the grid simply grows taller and scrolls.
///
/// The reference answers that case by stacking runs of alike coins into piles instead. That is not
/// ported: a stack of two reads as one oddly thick coin and answers a grouping question nobody
/// asked. Its vectors are still vendored, and the cases that stack are skipped rather than
/// checked.
enum CoinageGridLayout {
    struct Options: Equatable {
        var maxDiameter: CGFloat = 52
        /// The reference's own floor, and what any crowded grid is drawn at: the fit is a cliff
        /// rather than a slope, so a grid that overflows at all drops straight here. Fifty coins
        /// are drawn at 51 and two hundred at the floor, with nothing in between. Raising it to 40
        /// was tried and turned five hundred coins into five screens of scrolling rather than three.
        var minDiameter: CGFloat = 28
        var gap: CGFloat = 5
        var header: CGFloat = 34
        var blockGap: CGFloat = 14
        init() {}
    }

    /// One holding, as much of it as the packing needs.
    struct Item: Equatable {
        let id: String
        let exponent: Int16
        let partition: CoinageStripLayout.Partition
    }

    struct Cell: Equatable {
        let id: String
        let centre: CGPoint
    }

    struct Block: Equatable {
        let partition: CoinageStripLayout.Partition
        let top: CGFloat
        let count: Int
    }

    struct Layout: Equatable {
        let diameter: CGFloat
        let pitch: CGFloat
        let columns: Int
        let cells: [Cell]
        let blocks: [Block]
        let height: CGFloat
        let fits: Bool
    }

    static func layout(
        _ items: [Item],
        area: CGSize,
        options: Options = Options()
    ) -> Layout {
        let partitions = split(items)

        // Both searches below are over monotone choices, so they bisect: the same answer as trying
        // every option, in a handful of passes rather than dozens.
        let diameters = Int(options.maxDiameter - options.minDiameter) + 1
        let widest = firstFit(diameters) {
            attempt(
                diameter: options.maxDiameter - CGFloat($0),
                partitions: partitions,
                area: area,
                options: options
            ).fits
        }

        if widest < diameters {
            return attempt(
                diameter: options.maxDiameter - CGFloat(widest),
                partitions: partitions,
                area: area,
                options: options
            )
        }

        return attempt(
            diameter: options.minDiameter,
            partitions: partitions,
            area: area,
            options: options
        )
    }
}

// MARK: - Packing

private extension CoinageGridLayout {
    struct Partition {
        let partition: CoinageStripLayout.Partition
        var items: [Item]
    }

    static func split(_ items: [Item]) -> [Partition] {
        items.reduce(into: [Partition]()) { partitions, item in
            if partitions.last?.partition == item.partition {
                partitions[partitions.count - 1].items.append(item)
            } else {
                partitions.append(Partition(partition: item.partition, items: [item]))
            }
        }
    }

    /// Smallest index in `0 ..< count` that fits, given fitting is monotone; `count` if none does.
    static func firstFit(_ count: Int, _ fits: (Int) -> Bool) -> Int {
        var low = 0
        var high = count

        while low < high {
            let mid = (low + high) / 2

            if fits(mid) {
                high = mid
            } else {
                low = mid + 1
            }
        }

        return low
    }

    static func attempt(
        diameter: CGFloat,
        partitions: [Partition],
        area: CGSize,
        options: Options
    ) -> Layout {
        let pitch = diameter + options.gap
        let columns = max(1, Int((area.width - pitch / 2 + options.gap) / pitch))
        let rowHeight = pitch * 3.0.squareRoot() / 2

        var cells: [Cell] = []
        var blocks: [Block] = []
        var top: CGFloat = 0

        for (index, partition) in partitions.enumerated() {
            if index > 0 { top += options.blockGap }

            blocks.append(
                Block(partition: partition.partition, top: top, count: partition.items.count)
            )
            top += options.header

            let rows = Int(ceil(Double(partition.items.count) / Double(columns)))

            for (position, item) in partition.items.enumerated() {
                let row = position / columns
                let column = position % columns

                cells.append(
                    Cell(
                        id: item.id,
                        centre: CGPoint(
                            x: diameter / 2 + CGFloat(column) * pitch
                                + CGFloat(row % 2) * (pitch / 2),
                            y: top + diameter / 2 + CGFloat(row) * rowHeight
                        )
                    )
                )
            }

            top += rows > 0 ? CGFloat(rows - 1) * rowHeight + diameter : 0
        }

        return Layout(
            diameter: diameter,
            pitch: pitch,
            columns: columns,
            cells: cells,
            blocks: blocks,
            height: top,
            fits: top <= area.height
        )
    }
}
