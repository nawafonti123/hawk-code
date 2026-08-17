import { describe, expect, it } from "vitest";
import { resolveBrowserInput } from "./browser-input";

describe("resolveBrowserInput", () => {
  it("keeps complete web addresses", () => {
    expect(resolveBrowserInput("https://vercel.com/docs")).toBe(
      "https://vercel.com/docs",
    );
  });

  it("adds HTTPS to domains and local development addresses", () => {
    expect(resolveBrowserInput("vercel.com")).toBe("https://vercel.com/");
    expect(resolveBrowserInput("localhost:3000")).toBe(
      "http://localhost:3000/",
    );
  });

  it("turns words and sentences into a search", () => {
    expect(resolveBrowserInput("vercal")).toBe(
      "https://www.google.com/search?q=vercal",
    );
    expect(resolveBrowserInput("بحث عن HAWK")).toBe(
      "https://www.google.com/search?q=%D8%A8%D8%AD%D8%AB%20%D8%B9%D9%86%20HAWK",
    );
  });

  it("rejects unsupported protocols and empty input", () => {
    expect(resolveBrowserInput("file:///C:/secret.txt")).toBeNull();
    expect(resolveBrowserInput("   ")).toBeNull();
  });
});
