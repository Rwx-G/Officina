// What a component starts when it mounts, ended when it unmounts.

/** Starts each of `starts` and returns one stop that ends them all. A start
 *  that resolves after the stop is ended as soon as it resolves, so no
 *  listener or timer outlives the component that started it. */
export function startAll(...starts: (() => Promise<() => void>)[]): () => void {
  let stopped = false;
  const stops: (() => void)[] = [];
  for (const start of starts) {
    void start().then((stop) => {
      if (stopped) stop();
      else stops.push(stop);
    });
  }
  return () => {
    stopped = true;
    for (const stop of stops.splice(0)) stop();
  };
}
