import Foundation
import AVFoundation

protocol AudioSessionManaging {
    func registerActivities(_ activities: Set<AudioSessionActivity>, for target: AnyObject) throws
    func deregisterActivities(for target: AnyObject) throws
}

final class AudioSessionManager {
    struct OngoingActivity {
        weak var target: AnyObject?
        let activities: Set<AudioSessionActivity>

        func hasActivity(_ activity: AudioSessionActivity) -> Bool {
            activities.contains(activity)
        }
    }

    let session: AVAudioSession

    private var ongoingActivities: [OngoingActivity] = []
    private let mutex = NSLock()

    init(session: AVAudioSession = AVAudioSession.sharedInstance()) {
        self.session = session
    }
}

private extension AudioSessionManager {
    func clearEmptyTargets() {
        ongoingActivities.removeAll { $0.target == nil }
    }

    func updateSessionCategory() throws {
        clearEmptyTargets()

        if ongoingActivities.contains(where: { $0.hasActivity(.playback) }) {
            try setCategory(.playback)
        } else {
            guard session.category != .ambient else {
                return
            }

            try setCategory(.ambient)
        }
    }

    func setCategory(
        _ category: AVAudioSession.Category,
        mode: AVAudioSession.Mode? = nil,
        options: AVAudioSession.CategoryOptions = [],
    ) throws {
        guard
            session.category != category
            || (session.mode != (mode ?? .default))
            || (session.categoryOptions != options)
        else {
            return
        }

        if let mode, !options.isEmpty {
            try session.setCategory(category, mode: mode, options: options)
        } else if let mode {
            try session.setCategory(category, mode: mode)
        } else if !options.isEmpty {
            try session.setCategory(category, options: options)
        } else {
            try session.setCategory(category)
        }
    }
}

extension AudioSessionManager: AudioSessionManaging {
    func registerActivities(_ activities: Set<AudioSessionActivity>, for target: AnyObject) throws {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        ongoingActivities.removeAll { $0.target === target || $0.target == nil }
        ongoingActivities.append(.init(target: target, activities: activities))

        try updateSessionCategory()
    }

    func deregisterActivities(for target: AnyObject) throws {
        mutex.lock()

        defer {
            mutex.unlock()
        }

        ongoingActivities.removeAll { $0.target === target || $0.target == nil }

        try updateSessionCategory()
    }
}
