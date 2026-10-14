import AsyncExtensions
import Foundation
import FoundationExt
import os
import StructuredConcurrency

/// The pacing of one wait for inputs, and the time it runs on. Injected so tests drive every timeout
/// and window from a test clock instead of waiting on the wall clock.
struct InputWaitTiming: Sendable {
    /// How long a look holds out for every input once *some* of them are visible.
    ///
    /// Holding out is deliberate: an input still landing is the ordinary reason a look is incomplete,
    /// and building half of a group now would put the rest in a second call for nothing. Settling for
    /// the last look is equally deliberate — an input that never arrives must not hold up the ones that
    /// did.
    let holdOut: Duration

    /// How long one call waits while nothing it asks about is visible. Returning then costs the
    /// executor another call, and bounds how long a look that never arrives can hold a call open.
    let idleLimit: Duration

    /// The wall-clock time the deadline is checked against. A date rather than a clock instant, so a
    /// window opened on one launch means the same thing on the next.
    let dateProvider: any DateProviding

    static let production = InputWaitTiming(
        holdOut: .seconds(30),
        idleLimit: .seconds(300),
        dateProvider: NowDateProvider()
    )
}

/// What one wait for inputs established.
struct InputsLook<Key: Hashable & Sendable>: Sendable {
    let present: Set<Key>

    /// Whether any look was taken at all: only a look can say an input is absent.
    let looked: Bool

    /// When this look was returned. `presence` is a live subscription that emits on every change, so a
    /// later time than the last emission still describes what the chain holds.
    let takenAt: Date

    /// Proven absent past its `deadline`, which is the only thing that ends a transaction's rebuilds.
    func abandoned(_ input: Key, deadline: Date) -> Bool {
        looked && !present.contains(input) && takenAt >= deadline
    }
}

/// The inputs of `wanted` that `presence` shows — once every one of them is visible, once `holdOut` has
/// passed since some of them were, or once `deadline` passes after a look was taken.
///
/// `presence` reports the whole set it can see on each look; a look it cannot take must not be emitted,
/// so a failed read never erases what the chain last showed. Every call opens its own subscription,
/// which starts from what the chain holds now.
func awaitInputs<Key: Hashable & Sendable>(
    presence: AnyAsyncSequence<Set<Key>>,
    wanted: Set<Key>,
    deadline: Date,
    timing: InputWaitTiming
) async -> InputsLook<Key> {
    // A buffered channel fed by a pump task: `next()` is cancellation-safe, so a timed-out receive
    // drops no buffered look. The same shape the claim loop uses to read the chain.
    let looks = AsyncBufferedChannel<Set<Key>>()
    let pump = Task {
        do {
            for try await look in presence {
                looks.send(look)
            }
        } catch {}

        looks.finish()
    }

    defer { pump.cancel() }

    let latest = OSAllocatedUnfairLock<Set<Key>?>(initialState: nil)
    let iterator = looks.makeAsyncIterator()

    _ = try? await withTimeout(timing.idleLimit) {
        while let look = await iterator.next()?.intersection(wanted) {
            latest.withLock { $0 = look }

            // Everything is here, or enough is here to be worth holding out for the rest.
            if wanted.isSubset(of: look) { break }

            if !look.isEmpty {
                let held = await holdOut(iterator, wanted: wanted, first: look, timing: timing)
                latest.withLock { $0 = held }

                break
            }

            // Nothing visible and the window has closed: this look is the last word.
            if await timing.dateProvider.read() >= deadline { break }
        }
    }

    let seen = latest.withLock { $0 }

    return await InputsLook(present: seen ?? [], looked: seen != nil, takenAt: timing.dateProvider.read())
}

/// Waits a little longer for the inputs a first look was missing.
///
/// The newest look wins outright, even when it holds fewer inputs than the one before: a fork can take
/// one away, and building against the widest view ever seen would spend what is no longer there.
private func holdOut<Key: Hashable & Sendable>(
    _ looks: AsyncBufferedChannel<Set<Key>>.Iterator,
    wanted: Set<Key>,
    first: Set<Key>,
    timing: InputWaitTiming
) async -> Set<Key> {
    let latest = OSAllocatedUnfairLock<Set<Key>>(initialState: first)

    _ = try? await withTimeout(timing.holdOut) {
        while !wanted.isSubset(of: latest.withLock { $0 }) {
            guard let look = await looks.next()?.intersection(wanted) else { break }

            latest.withLock { $0 = look }
        }
    }

    return latest.withLock { $0 }
}
