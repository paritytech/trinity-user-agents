import Testing

/// Root of every Core Data benchmark suite. Serialized so scenarios never share the simulator's CPU
/// with each other; run them with `-only-testing:` so the rest of the integration target stays out too.
@Suite("CoreDataBenchmarks", .serialized)
enum CoreDataBenchmarks {}
