import CoreMotion
import Metal
import simd
import Testing

@testable import polkadot_app

/// The light cannot be tried in the simulator, which has no gyro, so the mapping from gravity to
/// where the studio sits is pinned here instead.
@Suite("Coin light tilt")
struct CoinageTiltTests {
    /// Which way is down for a phone leaned `lean` radians back from vertical, then turned `angle`
    /// radians about one of its own axes.
    ///
    /// Built by actually rotating gravity rather than by writing down its components, so the two
    /// gestures below differ the way they do in a hand.
    private func gravity(
        lean: Double = 0,
        turned angle: Double = 0,
        about axis: SIMD3<Double> = .init(0, 0, 1)
    ) -> CMAcceleration {
        let down = SIMD3(0, -cos(lean), -sin(lean))
        let unit = axis / max(length(axis), 1e-9)
        let turned = down * cos(-angle)
            + cross(unit, down) * sin(-angle)
            + unit * dot(unit, down) * (1 - cos(-angle))

        return CMAcceleration(x: turned.x, y: turned.y, z: turned.z)
    }

    /// A wrist rolling the phone in the plane of its own screen: the gesture that used to do
    /// nothing in one direction.
    private func rolled(_ angle: Double, _ lean: Double = 0.9) -> CMAcceleration {
        gravity(lean: lean, turned: angle, about: .init(0, 0, 1))
    }

    /// One long edge lifted off the table, the other gesture a hand calls tilting sideways.
    private func lifted(_ angle: Double, _ lean: Double = 0.9) -> CMAcceleration {
        gravity(lean: lean, turned: angle, about: .init(0, 1, 0))
    }

    private func pose(_ gravity: CMAcceleration) -> CoinageTilt.Pose {
        CoinageTilt.Pose(gravity: gravity)
    }

    private func magnitude(_ turn: CoinageTilt.Turn) -> Double { length(turn) }

    /// Feeds a pose for `seconds`, one reading per frame, as if the phone were held there.
    private func hold(_ tilt: CoinageTilt, at gravity: CMAcceleration, for seconds: Double) {
        for _ in 0 ..< Int(seconds * 60) {
            tilt.absorb(pose(gravity), after: 1.0 / 60)
        }
    }

    /// Feeds a sweep between two angles over `seconds`, as a hand actually moves.
    private func sweep(
        _ tilt: CoinageTilt,
        from start: Double,
        to end: Double,
        over seconds: Double,
        by gesture: (Double) -> CMAcceleration
    ) {
        let steps = Int(seconds * 60)

        for step in 1 ... steps {
            let angle = start + (end - start) * Double(step) / Double(steps)
            tilt.absorb(pose(gesture(angle)), after: 1.0 / 60)
        }
    }

    @Test("However the phone is first picked up, the light starts square")
    func firstReadingIsNeutral() {
        for start in [rolled(0), rolled(.pi / 5), gravity(lean: 1.1)] {
            let tilt = CoinageTilt()
            tilt.absorb(pose(start), after: 1.0 / 60)
            tilt.advance(by: 1)

            #expect(magnitude(tilt.turn) < 1e-6)
        }
    }

    @Test("Tilting one way and the other is the same movement mirrored, at any lean")
    func tiltingBothWaysIsSymmetric() {
        // The fault this replaces: a sideways movement turned the studio about the screen's long
        // axis, which slides the reflection along a wall of even brightness one way and across the
        // room the other. Measured over a coin's whole face, forty degrees changed it by 0.31 one
        // way and 0.10 the other; correcting the sense only swapped which hand was dead.
        for lean in [0.0, 0.5, 0.9, 1.4] {
            for gesture in [rolled, lifted] {
                let neutral = pose(gesture(0, lean))
                let left = neutral.turn(to: pose(gesture(-0.4, lean)))
                let right = neutral.turn(to: pose(gesture(0.4, lean)))

                #expect(abs(magnitude(left) - magnitude(right)) < 1e-9)
                #expect(abs(left.z + right.z) < 1e-9)
            }
        }
    }

    @Test("A sideways movement rolls the studio about the axis the coin faces")
    func sidewaysRollsAboutTheViewAxis() {
        // A roll about the axis a coin faces cannot care which way it went, which is what makes
        // the response symmetric.
        for lean in [0.0, 0.5, 0.9, 1.4] {
            // A roll in the plane of the screen leaves the lean alone, so it is pure roll.
            let turn = pose(rolled(0, lean)).turn(to: pose(rolled(0.4, lean)))

            #expect(magnitude(turn) > 1e-6)
            #expect(abs(turn.x) < 1e-9)
            #expect(abs(turn.y) < 1e-9)

            // Lifting an edge leans the phone as well, so that one carries a pitch besides. It
            // does nothing at all on an upright phone, where it turns about gravity itself.
            let lift = pose(lifted(0, lean)).turn(to: pose(lifted(0.4, lean)))

            #expect(abs(lift.z) > 1e-3 || lean < 0.3)
        }
    }

    @Test("Dipping an edge rolls the room the way the hand went")
    func rollFollowsTheHand() {
        let flat = pose(lifted(0, .pi / 2)).turn(to: pose(lifted(0.4, .pi / 2)))

        #expect(flat.z > 0)
    }

    @Test("Leaning back pitches the studio without rolling it")
    func leanIsPurePitch() {
        let back = pose(gravity(lean: 0)).turn(to: pose(gravity(lean: 0.4)))

        #expect(back.x > 0)
        #expect(abs(back.z) < 1e-9)
    }

    @Test("Held at a new angle, the light is square again within a couple of seconds")
    func heldBecomesSquare() {
        let tilt = CoinageTilt()
        tilt.absorb(pose(gravity(lean: 0)), after: 1.0 / 60)
        sweep(tilt, from: 0, to: .pi / 6, over: 0.4) { gravity(lean: $0) }
        tilt.advance(by: 1)

        #expect(magnitude(tilt.turn) > 0.4)

        hold(tilt, at: gravity(lean: .pi / 6), for: 3)
        tilt.advance(by: 1)

        #expect(magnitude(tilt.turn) < 0.05)
    }

    @Test("Going away and coming back leaves the resting position square")
    func returningIsSquareAgain() {
        let tilt = CoinageTilt()
        hold(tilt, at: rolled(0), for: 1)

        sweep(tilt, from: 0, to: -.pi / 4, over: 0.4) { rolled($0) }
        hold(tilt, at: rolled(-.pi / 4), for: 0.3)
        sweep(tilt, from: -.pi / 4, to: 0, over: 0.4) { rolled($0) }
        hold(tilt, at: rolled(0), for: 1.5)
        tilt.advance(by: 1)

        // The excursion must not have dragged the resting position with it.
        #expect(magnitude(tilt.turn) < 0.05)
    }

    @Test("A deliberate tilt reads in full rather than being followed")
    func movementIsNotChased() {
        let tilt = CoinageTilt()
        hold(tilt, at: gravity(lean: 0), for: 1)
        sweep(tilt, from: 0, to: .pi / 4, over: 0.5) { gravity(lean: $0) }
        tilt.advance(by: 1)

        #expect(abs(magnitude(tilt.turn) - CoinageTilt.travel) < 1e-6)
    }

    @Test("A tilt reaches the ends of the travel and goes no further")
    func travelIsBounded() {
        for angle in [CoinageTilt.range, .pi / 3, .pi / 2] {
            let tilt = CoinageTilt()
            hold(tilt, at: gravity(lean: 0), for: 1)
            sweep(tilt, from: 0, to: angle, over: 0.4) { gravity(lean: $0) }
            tilt.advance(by: 1)

            #expect(abs(magnitude(tilt.turn) - CoinageTilt.travel) < 1e-6)
        }
    }

    @Test("Leaning the phone right through flat never jumps")
    func leaningThroughFlatIsContinuous() {
        // The fault this replaces: a bearing between two shrinking components, which turned a five
        // degree roll into twenty-seven degrees leaned back and a hundred and seventy past flat.
        let neutral = pose(gravity(lean: 0))
        var previous = magnitude(neutral.turn(to: pose(gravity(lean: 0))))

        for step in 1 ... 40 {
            let current = magnitude(neutral.turn(to: pose(gravity(lean: Double(step) * 0.05))))

            #expect(current - previous > -1e-9)
            #expect(current - previous < 0.06)
            previous = current
        }
    }

    @Test("Flat on a table the light sits square rather than chasing noise")
    func flatIsSquare() {
        let flat = pose(CMAcceleration(x: 0, y: 0, z: -1))

        #expect(magnitude(flat.turn(to: flat)) < 1e-9)
    }
}

@Suite("Coin flight stagger")
struct CoinageStaggerTests {
    @Test("Leaving the strip, the first coin sets off first")
    func fromFrontLeadsWithTheFirst() {
        let stagger = CoinageCoinField.Stagger.fromFront

        #expect(stagger.position(of: 0, of: 10) == 0)
        #expect(stagger.position(of: 9, of: 10) == 9)
    }

    @Test("Spreading into the grid, the last coin sets off first")
    func fromBackLeadsWithTheLast() {
        let stagger = CoinageCoinField.Stagger.fromBack

        #expect(stagger.position(of: 9, of: 10) == 0)
        #expect(stagger.position(of: 0, of: 10) == 9)
    }

    @Test("Either way every coin takes its own place in the order")
    func everyCoinGetsOnePlace() {
        for stagger in [CoinageCoinField.Stagger.fromFront, .fromBack] {
            let places = (0 ..< 24).map { stagger.position(of: $0, of: 24) }

            #expect(Set(places) == Set(0 ..< 24))
        }
    }
}

/// The shader's parameter block is padded to its own alignment, and Metal on a device refuses a
/// buffer smaller than the struct it declares — it fails at the draw call and says nothing useful.
/// Cheaper to assert the size here than to find it on hardware.
@Suite("Coin shader parameters")
struct CoinageParamsTests {
    @Test("The parameter block is the size the shader's struct is padded to")
    func packedMatchesTheShaderStruct() throws {
        let store = try? CoinageAssetStore(device: MTLCreateSystemDefaultDevice()!)
        let packed = try #require(store?.params.packed(), "assets did not load")
        let bytes = packed.count * MemoryLayout<Float>.size

        #expect(bytes % CoinageAssetStore.Params.alignment == 0)
        // CoinParams in CoinageCoin.metal. Recheck with a static_assert in a scratch .metal file if
        // a field is added and this starts failing.
        #expect(bytes == 128)
    }
}
