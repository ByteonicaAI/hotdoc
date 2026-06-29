import "@testing-library/jest-dom/vitest";

// jsdom does not implement scrollIntoView. App.svelte's selection effect calls
// it on every selectedIndex change, so stub it to a no-op for the test env.
Element.prototype.scrollIntoView = () => {};

// jsdom does not implement matchMedia. Svelte's `prefersReducedMotion`
// (svelte/motion) evaluates it at import time, so provide a minimal stub that
// reports "no preference" (matches: false).
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}

// jsdom does not implement the Web Animations API. Svelte's `fade` transition
// (App.svelte's search/copied views) calls element.animate, so stub it to a
// no-op animation that finishes immediately, letting Svelte's transition
// cleanup run so elements still mount/unmount as the tests expect.
if (!Element.prototype.animate) {
  Element.prototype.animate = function animate() {
    const animation = {
      onfinish: null as (() => void) | null,
      oncancel: null as (() => void) | null,
      cancel() {},
      finish() {},
      pause() {},
      play() {},
      finished: Promise.resolve(),
    };
    // Svelte assigns onfinish synchronously after animate() returns; invoke it
    // on the next microtask so the transition is treated as complete.
    void Promise.resolve().then(() => animation.onfinish?.());
    return animation as unknown as Animation;
  };
}
