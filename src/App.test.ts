import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import App from "./App.svelte";

describe("App", () => {
  it("renders the welcome heading", () => {
    render(App);
    expect(
      screen.getByRole("heading", { name: /welcome to tauri \+ svelte/i }),
    ).toBeInTheDocument();
  });

  it("renders a greet form with an input and a button", () => {
    render(App);
    expect(screen.getByPlaceholderText(/enter a name/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /greet/i })).toBeInTheDocument();
  });
});
