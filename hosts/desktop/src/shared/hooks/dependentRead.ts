import { type ReadHookValue } from './useRead';

// The pending/error half of a read — enough to wait on, without caring what it read.
type Dependency = { pending: boolean; error: unknown };

// Composes a read that can only start once another read has produced its params.
//
// While the dependency is still loading, the dependent read has no params, so it
// never started — and an unstarted `useRead` reports `pending: false` with its
// default value. Consumers correctly read that as "settled with nothing" and render
// an empty or error state that then flips to real data a moment later.
//
// `enabled` is what the caller actually asked for. When nothing was requested there
// is nothing to wait on, and claiming pending would stall a caller that never wanted
// a read at all.
export function dependentRead<D>(
  result: ReadHookValue<D>,
  dependency: Dependency,
  { enabled }: { enabled: boolean },
): ReadHookValue<D> {
  if (!enabled) return result;

  return {
    ...result,
    pending: dependency.pending || result.pending,
    fulfilled: !dependency.pending && result.fulfilled,
    // The dependency failing is why this read never ran — surface that rather than
    // letting it read as an empty result.
    error: result.error ?? dependency.error,
  };
}
