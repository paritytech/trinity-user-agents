import Foundation

/// Folds the ordered `StallEvent` stream into the activity/step tree that `StalenessReport` publishes.
/// A single consumer owns it, so no synchronisation happens here.
struct StallActivityTree {
    fileprivate struct Step {
        let id: UUID
        let title: String
        let depth: Int
        var state: StallStep.State
        let startedAt: Date
        var finishedAt: Date?
    }

    fileprivate struct Activity {
        let id: UUID
        let title: String
        let subtitle: String?
        let startedAt: Date
        var endedAt: Date?
        var visibility: StallActivity.Visibility
        var steps: [Step]
        var open: Set<UUID>
        var dismissed: Bool
    }

    private let maxSteps: Int
    private var activities: [UUID: Activity] = [:]
    private var stepParents: [UUID: UUID] = [:]

    init(maxSteps: Int) {
        self.maxSteps = maxSteps
    }

    var visibleActivities: [StallActivity] {
        activities.values
            .filter { !$0.dismissed }
            .map { activity in
                StallActivity(
                    id: activity.id,
                    title: activity.title,
                    subtitle: activity.subtitle,
                    startedAt: activity.startedAt,
                    endedAt: activity.endedAt,
                    visibility: activity.visibility,
                    steps: activity.steps.map(makeStep)
                )
            }
            .sorted { $0.startedAt < $1.startedAt }
    }

    /// Returns whether the tree changed and subscribers need a fresh publication.
    mutating func apply(_ event: StallEvent) -> Bool {
        switch event {
        case let .activityStarted(id, title, at):
            startActivity(id: id, title: title, at: at)
        case let .regionStarted(id, parent, title, at):
            startRegion(id: id, parent: parent, title: title, at: at)
        case let .ended(id, at, outcome):
            end(id: id, at: at, outcome: outcome)
        case let .dismissed(id):
            dismiss(id: id)
        case .barrier:
            false
        }
    }
}

private extension StallActivityTree {
    func makeStep(_ step: Step) -> StallStep {
        StallStep(
            id: step.id.uuidString,
            title: step.title,
            depth: step.depth,
            state: step.state,
            startedAt: step.startedAt,
            finishedAt: step.finishedAt
        )
    }

    mutating func removeActivity(_ activity: Activity) {
        for step in activity.steps {
            stepParents.removeValue(forKey: step.id)
        }

        activities.removeValue(forKey: activity.id)
    }

    mutating func startActivity(id: UUID, title: String, at: Date) -> Bool {
        activities[id] = Activity(
            id: id,
            title: title,
            subtitle: nil,
            startedAt: at,
            endedAt: nil,
            visibility: .whenStale,
            steps: [],
            open: [id],
            dismissed: false
        )

        return true
    }

    mutating func startRegion(id: UUID, parent: UUID, title: String, at: Date) -> Bool {
        guard
            let activityId = activityId(owning: parent),
            var activity = activities[activityId],
            activity.endedAt == nil
        else {
            // A region that starts after its root ended belongs to work that outlived the activity,
            // and is dropped rather than reopening a finished tree.
            return false
        }

        // Read the depth before the cap runs: eviction may drop the parent step itself.
        let parentDepth = activity.steps.first { $0.id == parent }?.depth ?? -1

        guard makeRoomForStep(in: &activity) else {
            return false
        }

        activity.steps.append(
            Step(
                id: id,
                title: title,
                depth: parentDepth + 1,
                state: .running,
                startedAt: at,
                finishedAt: nil
            )
        )
        activity.open.insert(id)
        stepParents[id] = parent
        activities[activityId] = activity

        return true
    }

    mutating func end(id: UUID, at: Date, outcome: RegionOutcome) -> Bool {
        guard
            let activityId = activities.first(where: { $0.value.open.contains(id) })?.key,
            var activity = activities[activityId]
        else {
            return false
        }

        let suppressDetail = hasFailedDescendant(of: id, in: activity)
        let closing = openDescendants(of: id, in: activity)
        activity.open.subtract(closing)

        for index in activity.steps.indices where closing.contains(activity.steps[index].id) {
            activity.steps[index].state = closedState(
                for: activity.steps[index].id,
                endedId: id,
                outcome: outcome,
                suppressDetail: suppressDetail
            )
            activity.steps[index].finishedAt = at
        }

        if case .failed = outcome {
            activity.visibility = .immediate
        }

        if id == activityId {
            if activity.dismissed {
                removeActivity(activity)
            } else {
                activity.endedAt = at
                activities[activityId] = activity
            }
        } else {
            activities[activityId] = activity
        }

        return true
    }

    mutating func dismiss(id: UUID) -> Bool {
        guard var activity = activities[id] else {
            return false
        }

        if activity.endedAt != nil {
            removeActivity(activity)
        } else {
            activity.dismissed = true
            activities[id] = activity
        }

        return true
    }

    func activityId(owning regionId: UUID) -> UUID? {
        if activities[regionId] != nil {
            return regionId
        }

        return activities.first { $0.value.open.contains(regionId) }?.key
    }

    /// The cap evicts the oldest closed step. When every step is still running there is nothing safe
    /// to evict, so the incoming region is dropped instead.
    mutating func makeRoomForStep(in activity: inout Activity) -> Bool {
        guard activity.steps.count >= maxSteps else {
            return true
        }

        guard let index = activity.steps.firstIndex(where: { $0.state != .running }) else {
            return false
        }

        let removed = activity.steps.remove(at: index)
        stepParents.removeValue(forKey: removed.id)

        return true
    }

    /// `id` plus every still-open step underneath it.
    func openDescendants(of id: UUID, in activity: Activity) -> Set<UUID> {
        var closing: Set<UUID> = [id]
        var pending = [id]

        while let current = pending.popLast() {
            let children = activity.steps.filter {
                activity.open.contains($0.id) && stepParents[$0.id] == current
            }

            for child in children where !closing.contains(child.id) {
                closing.insert(child.id)
                pending.append(child.id)
            }
        }

        return closing
    }

    /// The same error unwinds through every enclosing region, so only the deepest failure carries the
    /// message; ancestors show state alone.
    func hasFailedDescendant(of id: UUID, in activity: Activity) -> Bool {
        activity.steps.contains { step in
            guard case .failed = step.state, step.id != id else {
                return false
            }

            return isDescendant(step.id, of: id)
        }
    }

    func isDescendant(_ stepId: UUID, of ancestorId: UUID) -> Bool {
        var cursor = stepParents[stepId]

        while let current = cursor {
            if current == ancestorId {
                return true
            }

            cursor = stepParents[current]
        }

        return false
    }

    func closedState(
        for stepId: UUID,
        endedId: UUID,
        outcome: RegionOutcome,
        suppressDetail: Bool
    ) -> StallStep.State {
        guard case let .failed(error) = outcome, stepId == endedId else {
            return .finished
        }

        return .failed(detail: suppressDetail ? nil : failureDetail(for: error))
    }

    /// `localizedDescription` on a pure-Swift error yields the useless NSError bridge string
    /// ("The operation couldn't be completed…"), so prefer an explicit `LocalizedError` description and
    /// otherwise dump the value, which at least carries its payload.
    func failureDetail(for error: Error) -> String {
        if let localized = error as? LocalizedError, let description = localized.errorDescription {
            return description
        }

        return String(describing: error)
    }
}
