import CoreGraphics
import Foundation
@testable import PolkadotUI
import SwiftUI
import Testing

private typealias Emitter = CustomMessageWidgetNode.Emitter

/// Particle emitters: the core's place rule, and when a field sends, keeps and drops particles.
struct ParticleTests {
    private static let dot = Emitter.Look.circle(diameter: 1, color: .white)

    private static func field() -> ParticleField {
        var field = ParticleField()
        field.random = { 0.5 }
        return field
    }

    @Test("A particle moves, falls, fades and shrinks over its life, as the core's Emitter::place_at")
    func place() throws {
        let falling = Emitter(looks: [Self.dot], source: .area, life: 2, gravity: 200, fadeFrom: 0.5, endScale: 0)
        let still = falling.place(start: CGPoint(x: 10, y: 20), speed: 0, direction: 0, age: 0)
        #expect(still == .init(point: CGPoint(x: 10, y: 20), opacity: 1, scale: 1))
        #expect(falling.place(start: .zero, speed: 0, direction: 0, age: 1) == .init(
            point: CGPoint(x: 0, y: 100), opacity: 1, scale: 0.5
        ))
        #expect(falling.place(start: .zero, speed: 1_000, direction: 0, age: 1.5) == .init(
            point: CGPoint(x: 1_500, y: 225), opacity: 0.5, scale: 0.25
        ))
        #expect(falling.place(start: .zero, speed: 0, direction: 0, age: 2) == nil, "gone at the end of its life")
        let down = try #require(
            Emitter(looks: [Self.dot], source: .area, life: 1).place(
                start: .zero,
                speed: 1_000,
                direction: .pi / 2,
                age: 0.5
            )
        )
        #expect(abs(down.point.y - 500) < 1e-9, "straight down at 1000 a second")
        let unfaded = Emitter(looks: [Self.dot], source: .area, life: 1, fadeFrom: 1)
        #expect(unfaded.place(start: .zero, speed: 0, direction: 0, age: 0.999)?.opacity == 1)
        #expect(Emitter(looks: [Self.dot], source: .area, life: 1, maxAlive: 4_000).maxAlive == 512)
    }

    @Test("A point source sends its rate a second, carrying parts of a particle between frames")
    func rate() {
        let emitter = Emitter(looks: [Self.dot], source: .point(CGPoint(x: 3, y: 4)), ratePerSecond: 10, life: 10)
        var field = Self.field()
        field.tick(emitter, sending: true, at: 0, finger: nil, size: CGSize(width: 10, height: 10))
        for frame in 1 ... 61 {
            field.tick(emitter, sending: true, at: Double(frame) / 60, finger: nil, size: CGSize(width: 10, height: 10))
        }
        #expect(field.particles.count == 10)
        #expect(field.particles.allSatisfy { $0.start == CGPoint(x: 3, y: 4) })
    }

    @Test("At most maxAlive particles live at once; the rest are not sent")
    func mostAlive() {
        let emitter = Emitter(looks: [Self.dot], source: .area, life: 10, maxAlive: 3)
        var field = Self.field()
        field.burst(emitter, count: 10, at: 0, finger: nil, size: CGSize(width: 20, height: 10))
        #expect(field.particles.count == 3)
        #expect(field.particles.allSatisfy { $0.start == CGPoint(x: 10, y: 5) }, "anywhere in the node")
    }

    @Test("Particles die at the end of their life, and nothing is sent while not sending")
    func life() {
        let emitter = Emitter(looks: [Self.dot], source: .area, ratePerSecond: 100, life: 1)
        var field = Self.field()
        field.burst(emitter, count: 2, at: 0, finger: nil, size: .zero)
        field.tick(emitter, sending: false, at: 0.5, finger: nil, size: .zero)
        #expect(field.particles.count == 2)
        field.tick(emitter, sending: false, at: 1, finger: nil, size: .zero)
        #expect(field.isEmpty)
    }

    @Test("A pan source sends while the finger is down, spaced along the way it went, and bursts where it last was")
    func pan() {
        let emitter = Emitter(looks: [Self.dot], source: .pan("pad"), ratePer100Points: 10, life: 10)
        var field = Self.field()
        field.tick(emitter, sending: true, at: 0, finger: CGPoint(x: 0, y: 0), size: .zero)
        field.tick(emitter, sending: true, at: 0.016, finger: CGPoint(x: 25, y: 0), size: .zero)
        #expect(field.particles.map(\.start) == [CGPoint(x: 10, y: 0), CGPoint(x: 20, y: 0)])
        field.tick(emitter, sending: true, at: 0.032, finger: CGPoint(x: 35, y: 0), size: .zero)
        #expect(field.particles.map(\.start.x) == [10, 20, 30], "the distance left over carries")
        field.tick(emitter, sending: true, at: 0.048, finger: nil, size: .zero)
        field.tick(emitter, sending: true, at: 0.064, finger: nil, size: .zero)
        #expect(field.particles.count == 3, "nothing while the finger is up")
        field.burst(emitter, count: 2, at: 0.08, finger: nil, size: .zero)
        #expect(field.particles.suffix(2).allSatisfy { $0.start == CGPoint(x: 35, y: 0) })
    }

    @Test("Frames that start again after a pause owe nothing for the time between")
    func resume() {
        let emitter = Emitter(looks: [Self.dot], source: .area, ratePerSecond: 10, life: 0.1)
        var field = Self.field()
        field.tick(emitter, sending: true, at: 0, finger: nil, size: .zero)
        field.resume()
        field.tick(emitter, sending: true, at: 100, finger: nil, size: .zero)
        #expect(field.isEmpty)
    }

    @Test("A particle takes one of the looks, its speed and spin between their ends, and its direction in the spread")
    func picks() {
        let emitter = Emitter(
            looks: [Self.dot, .rectangle(size: CGSize(width: 2, height: 1), color: .red)],
            source: .area,
            life: 1,
            speed: 100 ... 300,
            angle: .pi / 2,
            spread: .pi,
            spin: -90 ... 90
        )
        var field = ParticleField()
        field.random = { 0.999 }
        field.burst(emitter, count: 1, at: 0, finger: nil, size: .zero)
        let particle = field.particles[0]
        #expect(particle.look == 1)
        #expect(abs(particle.speed - 299.8) < 1e-9)
        #expect(abs(particle.direction - (.pi / 2 + 0.499 * .pi)) < 1e-9)
        #expect(abs(particle.spin - 89.82) < 1e-9)
    }
}
