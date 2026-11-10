import { useEffect, useRef, useState } from 'react';

// Tracks a single element's measured height via ResizeObserver. Height-only by
// design — the sole consumer (the chat widget) drives layout off vertical space.
// Add a width field here if a future consumer needs it.
export function useElementSize<T extends HTMLElement = HTMLDivElement>() {
  const ref = useRef<T>(null);
  const [height, setHeight] = useState(0);

  useEffect(() => {
    const element = ref.current;
    if (!element) return;

    const update = () => setHeight(element.clientHeight);

    update();

    const resizeObserver = new ResizeObserver(update);
    resizeObserver.observe(element);

    return () => resizeObserver.disconnect();
  }, []);

  return { ref, height };
}
