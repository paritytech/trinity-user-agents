import { Subscription, firstValueFrom } from 'rxjs';

import { environmentUseCase } from '@/domains/application';
import { executableArchiveResource, productsResource, resolveProductUseCase } from '@/domains/product';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { lifecycleUseCase } from './lifecycleUseCase';
import { createWorkerFetchResolver } from './workerFetchResolver';

// Live worker subscriptions keyed by product id. Each one IS its instance's lifetime
// (see `lifecycleUseCase.createInstance$`), so unsubscribing disposes the worker.
//
// Keyed by product id, not contenthash: the core's ledger counts per product, and two
// products publishing byte-identical worker archives are still two products.
//
// RxJS subscription handles rather than `state/`: they are not observable state and
// nothing outside this file can act on them. What IS observable — the running
// instances — lives in `state/registry.ts`.
// A slot is claimed the moment demand arrives and carries a generation, because
// `start` awaits three times before it has anything to put there. Demand can bounce
// false→true across those awaits — `WorkersManager` re-keys every holder on an account
// switch, which is exactly that — leaving two starts in flight for one product. The
// generation is what lets a resumed start tell "my slot" from "the slot that replaced
// mine": without it the loser overwrites the winner's subscription without
// unsubscribing, and its sandbox, core provider and chat delivery leak for the session.
//
// `target` is the worker contenthash this slot was launched to run. It is what a
// redeploy is measured against — deliberately the bytes we AIMED for, not the ones we
// ended up building. Comparing against what was built would restart forever whenever
// the resolved row lags the observed one.
type Slot = { generation: number; target: string | null; subscription: Subscription };

const running = new Map<string, Slot>();
let nextGeneration = 0;

// The newest worker contenthash the live product rows have reported, per product.
// Kept whether or not the product is running — a row can change while no worker is up,
// and the value is then already correct when one starts.
const latestWorkerHash = new Map<string, string>();

/** Drop the slot only if it is still the one this generation claimed. */
function stopIfCurrent(productId: string, generation: number): void {
  const slot = running.get(productId);
  if (slot?.generation !== generation) return;

  slot.subscription.unsubscribe();
  running.delete(productId);
}

async function start(productId: string, generation: number): Promise<void> {
  const isCurrent = () => running.get(productId)?.generation === generation;

  const product = await resolveProductUseCase.resolveProduct(productId);
  if (!isCurrent()) return;

  if (!product?.executables.worker) {
    // The core may want a worker the host cannot supply — an uninstalled product, or
    // one whose manifest declares none. Not an error: nothing to run, so nothing runs.
    // The slot is released so a later acquire retries rather than being swallowed by
    // the "already running" guard forever.
    console.debug('[product-workers] demand for a product with no worker executable', { productId });
    stopIfCurrent(productId, generation);

    return;
  }

  const { entrypoint, contenthash: workerHash } = product.executables.worker;
  const { ipfsGatewayUrl } = await environmentUseCase.getActive();
  if (!isCurrent()) return;

  const content = await firstValueFrom(executableArchiveResource.read$({ product, kind: 'worker', ipfsGatewayUrl }));
  if (!isCurrent()) return;

  if (!content) {
    // Typically a transient fetch failure, so releasing the slot is what makes the
    // next acquire able to try again.
    console.warn('[product-workers] worker archive unavailable', { productId });
    stopIfCurrent(productId, generation);

    return;
  }

  let failedSynchronously = false;
  const subscription = lifecycleUseCase
    .createInstance$({
      contenthash: content.contenthash,
      files: content.archive.files,
      entrypoint,
      fetchResolver: createWorkerFetchResolver(productId),
      getProduct: () => product,
    })
    .subscribe({
      // A build failure terminates the stream. Releasing the slot is the point: a dead
      // subscription left in the map reads as "running" and wedges the product for the
      // rest of the session.
      error: (error: unknown) => {
        failedSynchronously = true;
        console.error('[product-workers] worker failed', { productId, error });
        stopIfCurrent(productId, generation);
      },
    });

  // The error handler above already released the slot; adding this subscription back
  // would re-register a stream that has already terminated.
  if (failedSynchronously) return;

  if (!isCurrent()) {
    subscription.unsubscribe();

    return;
  }

  // Only when the launch had no observed hash to aim at — the product stream had not
  // emitted yet. What we built is then the best available truth, and it keeps a first
  // emission from reading as a redeploy and bouncing a worker that is already correct.
  const slot = running.get(productId);
  running.set(productId, { generation, target: slot?.target ?? workerHash, subscription });

  // The row can change while the three awaits above are in flight, so the landing
  // start re-checks rather than trusting the snapshot it started from.
  restartIfStale(productId);
}

/**
 * Restart a running worker whose product has been redeployed.
 *
 * Nothing else would: the core's demand never drops across a redeploy — `WorkersManager`
 * deliberately keeps one holder so a commit does not bounce the worker — and `start`
 * snapshots the archive once. Without this the product runs its old bytes until the
 * next launch.
 */
function restartIfStale(productId: string): void {
  const slot = running.get(productId);
  if (!slot) return;

  const latest = latestWorkerHash.get(productId);
  if (!latest || latest === slot.target) return;

  stop(productId);
  launch(productId);
}

/** Claim the slot and begin a start against it. */
function launch(productId: string): void {
  // Claimed before the async start so a second demand for the same product cannot
  // race a second instance in behind it.
  const generation = ++nextGeneration;
  running.set(productId, { generation, target: latestWorkerHash.get(productId) ?? null, subscription: new Subscription() });

  void start(productId, generation).catch((error: unknown) => {
    console.error('[product-workers] worker start failed', { productId, error });
    stopIfCurrent(productId, generation);
  });
}

function stop(productId: string): void {
  running.get(productId)?.subscription.unsubscribe();
  running.delete(productId);
}

/**
 * Run the product workers the core says are wanted, and stop the ones it does not.
 *
 * The core is the ledger: a mounted surface takes a reference through `acquireWorker`,
 * and so does an open render stream, so this is the only place that knows the full
 * demand. The host obeys it rather than deciding from what happens to be mounted —
 * which is what lets a product draw a chat message while none of its UI is on screen.
 *
 * Returns a teardown that drops the listener and every running worker.
 */
async function watchDemand(): Promise<VoidFunction> {
  const runtime = await truapiRuntimeUseCase.whenRuntimeReady();

  const unsubscribe = runtime.subscribeWorkerDemand(({ productId, wanted }) => {
    if (!wanted) {
      stop(productId);

      return;
    }

    if (running.has(productId)) return;

    launch(productId);
  });

  // The committed row is what a redeploy rewrites, and this resource is the live DB,
  // so an update lands here without anything having to poll for it.
  const redeploys = productsResource.read$({}).subscribe(products => {
    for (const product of products) {
      const contenthash = product.executables.worker?.contenthash;
      if (!contenthash) continue;

      latestWorkerHash.set(product.baseName, contenthash);
      restartIfStale(product.baseName);
    }
  });

  return () => {
    unsubscribe();
    redeploys.unsubscribe();
    latestWorkerHash.clear();
    for (const productId of [...running.keys()]) stop(productId);
  };
}

export const productWorkersUseCase = {
  watchDemand,
};
