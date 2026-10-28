// Browser APIs jsdom lacks, stubbed for every Vitest case (vitest.config.ts
// `setupFiles`). Reduced motion is on by default, as the safest reading of
// a test that does not say; a case that needs motion stubs `matchMedia`
// itself. Imported by Vitest only.

if (typeof window !== "undefined") {
  // Node's own `localStorage` global shadows jsdom's, warns without a file
  // and keeps nothing: the window's storage is jsdom's.
  const dom = (globalThis as { jsdom?: { window: Window } }).jsdom;
  if (dom) Object.defineProperty(globalThis, "localStorage", { value: dom.window.localStorage, configurable: true });

  if (!window.matchMedia) {
    window.matchMedia = (query: string): MediaQueryList => ({
      matches: /prefers-reduced-motion:\s*reduce/.test(query),
      media: query,
      onchange: null,
      addListener: () => undefined,
      removeListener: () => undefined,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      dispatchEvent: () => false,
    });
  }

  if (!("ResizeObserver" in globalThis)) {
    globalThis.ResizeObserver = class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    };
  }

  // A finished, effect-less animation: what Svelte's transitions await.
  if (!Element.prototype.animate) {
    Element.prototype.animate = function (): Animation {
      return {
        currentTime: 0,
        effect: null,
        finished: Promise.resolve(),
        onfinish: null,
        cancel: () => undefined,
        finish: () => undefined,
        pause: () => undefined,
        play: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      } as unknown as Animation;
    };
  }

  if (!Element.prototype.getAnimations) {
    Element.prototype.getAnimations = () => [];
  }
}
