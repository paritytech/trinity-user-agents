import Testing
import Foundation
@testable import Coinage

struct RecyclerFungibilityTests {
    // MARK: - maximum

    @Test("An untouched ring is fully fungible")
    func maximumWithNothingUnloaded() {
        #expect(RecyclerFungibility.maximum(included: 64, unloaded: 0, capacity: 64) == 100)
    }

    @Test("A fully drained ring offers nothing")
    func maximumWithEverythingUnloaded() {
        #expect(RecyclerFungibility.maximum(included: 64, unloaded: 64, capacity: 64) == 0)
    }

    @Test(
        "maximum follows 100·(L²−U²)/L²",
        arguments: [
            (unloaded: UInt32(32), capacity: 64, expected: UInt8(75)),
            (unloaded: UInt32(16), capacity: 64, expected: UInt8(94)),
            (unloaded: UInt32(48), capacity: 64, expected: UInt8(44)),
            (unloaded: UInt32(5), capacity: 10, expected: UInt8(75))
        ]
    )
    func maximumFormula(unloaded: UInt32, capacity: Int, expected: UInt8) {
        // A full ring, so U is not clamped and the formula is exercised as written.
        #expect(
            RecyclerFungibility.maximum(
                included: UInt32(capacity),
                unloaded: unloaded,
                capacity: capacity
            ) == expected
        )
    }

    /// The ceiling is frozen, so a transiently over-large unloaded count must not depress it
    /// permanently: it is sanitised against the members it refers to, exactly as `current` is.
    @Test("An unloaded count past the included count clamps to it, not to capacity")
    func maximumClampsUnloadedToIncluded() {
        // 100·(64² − 8²)/64² = 98.4, not the 61 that clamping to capacity would give.
        #expect(RecyclerFungibility.maximum(included: 8, unloaded: 40, capacity: 64) == 98)
    }

    @Test("An empty ring scores zero")
    func maximumWithEmptyRing() {
        #expect(RecyclerFungibility.maximum(included: 0, unloaded: 0, capacity: 64) == 0)
    }

    @Test("A capacity of zero scores zero rather than dividing by zero")
    func maximumWithoutCapacity() {
        #expect(RecyclerFungibility.maximum(included: 4, unloaded: 0, capacity: 0) == 0)
    }

    // MARK: - current

    @Test("A full ring with nothing unloaded matches the maximum")
    func currentAtFullRing() {
        #expect(RecyclerFungibility.current(included: 64, unloaded: 0, capacity: 64) == 100)
    }

    @Test(
        "current follows 100·(I²−U²)/(I·L)",
        arguments: [
            // Half-filled, untouched: 100·(32²−0)/(32·64) = 50
            (included: UInt32(32), unloaded: UInt32(0), capacity: 64, expected: UInt8(50)),
            // Half-filled, half of those unloaded: 100·(1024−256)/2048 = 37.5 -> 38
            (included: UInt32(32), unloaded: UInt32(16), capacity: 64, expected: UInt8(38)),
            // Everything included has been unloaded
            (included: UInt32(32), unloaded: UInt32(32), capacity: 64, expected: UInt8(0)),
            (included: UInt32(10), unloaded: UInt32(0), capacity: 10, expected: UInt8(100))
        ]
    )
    func currentFormula(included: UInt32, unloaded: UInt32, capacity: Int, expected: UInt8) {
        #expect(
            RecyclerFungibility.current(included: included, unloaded: unloaded, capacity: capacity) == expected
        )
    }

    @Test("An unloaded count past the included count clamps instead of going negative")
    func currentClampsUnloadedToIncluded() {
        #expect(RecyclerFungibility.current(included: 8, unloaded: 40, capacity: 64) == 0)
    }

    @Test("An empty ring scores zero rather than dividing by zero")
    func currentWithEmptyRing() {
        #expect(RecyclerFungibility.current(included: 0, unloaded: 0, capacity: 64) == 0)
    }

    @Test("A capacity of zero scores zero rather than dividing by zero")
    func currentWithoutCapacity() {
        #expect(RecyclerFungibility.current(included: 4, unloaded: 0, capacity: 0) == 0)
    }

    // MARK: - Invariants

    /// The bar geometry in the details view assumes `current <= maximum` for the same reading, so
    /// that the barber-pole segment has a non-negative length.
    @Test("current never exceeds maximum for the same reading")
    func currentNeverExceedsMaximum() {
        for capacity in [4, 10, 16, 64, 128] {
            for included in 0 ... capacity {
                for unloaded in 0 ... capacity {
                    let current = RecyclerFungibility.current(
                        included: UInt32(included),
                        unloaded: UInt32(unloaded),
                        capacity: capacity
                    )
                    let maximum = RecyclerFungibility.maximum(
                        included: UInt32(included),
                        unloaded: UInt32(unloaded),
                        capacity: capacity
                    )

                    #expect(current <= maximum, "I=\(included) U=\(unloaded) L=\(capacity)")
                }
            }
        }
    }

    @Test("Both scores stay on the 0...100 scale for every reachable reading")
    func scoresStayOnScale() {
        for capacity in [1, 3, 10, 64] {
            for included in 0 ... (capacity * 2) {
                for unloaded in 0 ... (capacity * 2) {
                    let current = RecyclerFungibility.current(
                        included: UInt32(included),
                        unloaded: UInt32(unloaded),
                        capacity: capacity
                    )
                    let maximum = RecyclerFungibility.maximum(
                        included: UInt32(included),
                        unloaded: UInt32(unloaded),
                        capacity: capacity
                    )

                    #expect(current <= 100)
                    #expect(maximum <= 100)
                }
            }
        }
    }
}
