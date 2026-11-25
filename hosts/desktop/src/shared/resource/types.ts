import { type Observable } from 'rxjs';
import { type Brand } from 'valibot';

type KeyValues = string | number | undefined | null;

export type KeyFn<Params> = (params: Params) => KeyValues | (KeyValues | KeyValues[])[];

export type NormalizedKeyFn<Params> = (params: Params) => ResourceKey;

export type MapCacheFn<Params, Response, Cache> = (cache: Cache, result: Response, params: Params) => Cache;

export type DefaultCache<Response> = Record<ResourceKey, Response>;

export type ResourceKey = string & Brand<'ResourceKey'>;

/**
 * Test seam: swap a resource's request implementation in place.
 *
 * Mixed into what a builder returns rather than declared on {@link Resource}, so
 * each builder can state the exact function shape it accepts — a query takes a
 * value or promise, a stream takes an Observable.
 */
export type Overridable<Fn> = {
  /**
   * Replaces the request implementation and **invalidates the cache**, so the
   * next read actually reaches the replacement instead of being served a value
   * the previous implementation produced earlier.
   *
   * Outranks the `mock` the builder declared: the order is `instead` > `mock` >
   * the real request. Cleared by `resetExecutionEnvironment()` from
   * `@/shared/execution-environment`, which `vitest.setup.js` calls after every
   * test — a spec never has to undo this itself — after which the resource falls
   * back to its `mock`, or to the real request if it declared none.
   */
  instead(fn: Fn): void;
};

export type Resource<Params, Response, Cache> = {
  /**
   * Generate unique key for resource read operation.
   */
  key: NormalizedKeyFn<Params>;
  /**
   * Calling resource read. The param is omittable only when `{}` satisfies `Params` (a
   * param-less resource, keyed as the empty params `{}`); a resource whose `Params` has
   * required fields still demands the argument, so `read$()` on it stays a compile error.
   */
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type -- `{} extends Params` is the type-level test for "Params has no required field"; `object`/`unknown` would not express it
  read$(...args: {} extends Params ? [params?: Params] : [params: Params]): Observable<Response>;
  /**
   * Return pending observable for given params.
   */
  pending$(params: Params): Observable<Response> | null;
  /**
   * In-memory cache for resource results.
   */
  cache$: Observable<Cache>;

  /**
   * Registers a callback function to be invoked when a read operation occurs.
   *
   * @param callback The function to be called when a read operation occurs. Receives an Observable of the response and the operation parameters.
   * @return A function that when called will unregister the callback and stop receiving read notifications.
   */
  onRead(callback: (observable: Observable<Response>, params: Params) => void): VoidFunction;

  /**
   * Invalidates cached data for the given params, forcing the next read to re-fetch.
   * Returns the cache snapshot before invalidation.
   */
  invalidate(params: Partial<Params>): Cache;

  /**
   * Clears all cached data, forcing subsequent reads to re-fetch from source.
   */
  invalidateAll(): void;

  /**
   * Returns the current cache value synchronously.
   * Prefer this over casting cache$ to BehaviorSubject.
   */
  snapshot(): Cache;
};

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyResource<Params = any, Response = any, Cache = any> = Resource<Params, Response, Cache>;
