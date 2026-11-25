import { type RefObject, useEffect, useRef } from 'react';

const DURATION_MS = 340;
// Overshoots the target and settles back — the bounce.
const EASING = 'cubic-bezier(0.34, 1.4, 0.64, 1)';

type AnimatedHeight = {
  /** Clips and animates. Give it `overflow: hidden`. */
  containerRef: RefObject<HTMLDivElement | null>;
  /** Wraps the content whose natural height drives the animation. */
  contentRef: RefObject<HTMLDivElement | null>;
};

/**
 * Animates a container to its content's height whenever that height changes.
 *
 * CSS cannot do this on its own. `interpolate-size: allow-keywords` only fires
 * when the *specified* value changes — `0` to `auto` — so it covers a collapse
 * but not a container that is `auto` in both states and merely has different
 * content. That second case is the common one for a list that fills in as
 * results arrive, and it is why this needs a ResizeObserver.
 *
 * The observer watches the content and the animation runs on the container, so
 * the animation cannot retrigger the observer that scheduled it. The keyframes
 * are not filled: the container is left at its natural height once the animation
 * ends, so nothing here pins a stale pixel value.
 */
export function useAnimatedHeight(): AnimatedHeight {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const running = useRef<Animation | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (!container || !content) return;
    // happy-dom ships neither of these, and a decorative height animation must
    // not be the reason a component fails to mount.
    if (typeof ResizeObserver === 'undefined' || typeof container.animate !== 'function') return;

    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    const observer = new ResizeObserver(entries => {
      const next = entries[0]?.contentRect.height;
      if (next === undefined || reduced) return;

      // The container's *current* height, not the last one observed: a round
      // resizes once per arriving answer, so the previous animation is usually
      // still mid-flight and its rendered height is where this one has to start.
      const from = container.getBoundingClientRect().height;
      if (from === next) return;

      // One animation at a time. Left to stack, each in-flight `height` tween
      // forces its own layout of the whole panel every frame, for as many
      // answers as the round has.
      running.current?.cancel();
      running.current = container.animate([{ height: `${from}px` }, { height: `${next}px` }], {
        duration: DURATION_MS,
        easing: EASING,
      });
    });

    observer.observe(content);

    return () => {
      observer.disconnect();
      running.current?.cancel();
    };
  }, []);

  return { containerRef, contentRef };
}
