import "@testing-library/jest-dom/vitest";

// jsdom does not implement scrollIntoView. App.svelte's selection effect calls
// it on every selectedIndex change, so stub it to a no-op for the test env.
Element.prototype.scrollIntoView = () => {};
