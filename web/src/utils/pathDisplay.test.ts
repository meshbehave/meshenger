import { describe, it, expect } from "vitest";
import { buildPathSegs, buildFullText, GAP } from "./pathDisplay";

// Helper: extract just the text labels from segs for compact assertions.
const texts = (segs: ReturnType<typeof buildPathSegs>["segs"]) =>
  segs.map((s) => s.text);

// Helper: indices of dim segs (gap indicators / truncation markers).
const dimAt = (segs: ReturnType<typeof buildPathSegs>["segs"]) =>
  segs.flatMap((s, i) => (s.dim ? [i] : []));

describe("buildPathSegs", () => {
  // ── Observed (no hop data at all) ──────────────────────────────────────────

  it("observed – no relay data: src → ··· → dst", () => {
    const { segs, didTruncate } = buildPathSegs([], [], "A", "B", false);
    expect(texts(segs)).toEqual(["A", "···", "B"]);
    expect(dimAt(segs)).toEqual([1]);
    expect(didTruncate).toBe(false);
  });

  // ── Request-only (partial, fits within maxVisible) ─────────────────────────

  it("request-only short: src → relay → ··· → dst", () => {
    const { segs, didTruncate } = buildPathSegs(["r1"], [], "A", "B", false);
    expect(texts(segs)).toEqual(["A", "r1", "···", "B"]);
    expect(dimAt(segs)).toEqual([2]);
    expect(didTruncate).toBe(false);
  });

  it("request-only at relay budget (4): fits with no truncation", () => {
    // 4 relays: segs = [src, r1, r2, r3, r4, ···, dst] = 7 = maxVisible
    const { segs, didTruncate } = buildPathSegs(
      ["r1", "r2", "r3", "r4"],
      [],
      "A",
      "B",
      false,
    );
    expect(texts(segs)).toEqual(["A", "r1", "r2", "r3", "r4", "···", "B"]);
    expect(didTruncate).toBe(false);
  });

  // ── Request-only truncated ─────────────────────────────────────────────────

  it("request-only truncated: src → first → ··· → last(our node) → ··· → dst", () => {
    // The specific JSON case: 7 request relays, a1ce is last.
    const reqHops = ["!54084240", "0da0", "TTSH", "tm21", "8ef3", "🏙", "a1ce"];
    const { segs, didTruncate } = buildPathSegs(
      reqHops,
      [],
      "!db2a8f9c",
      "6a44",
      false,
    );
    expect(texts(segs)).toEqual([
      "!db2a8f9c",
      "!54084240", // first relay (context)
      "···", // hidden middle relays
      "a1ce", // last relay = our node
      "···", // unknown hops to dst
      "6a44",
    ]);
    expect(dimAt(segs)).toEqual([2, 4]);
    expect(didTruncate).toBe(true);
  });

  it("request-only truncated (5 relays, 1 over budget): first and last shown", () => {
    const { segs, didTruncate } = buildPathSegs(
      ["r1", "r2", "r3", "r4", "r5"],
      [],
      "A",
      "B",
      false,
    );
    expect(texts(segs)).toEqual(["A", "r1", "···", "r5", "···", "B"]);
    expect(didTruncate).toBe(true);
  });

  // ── Round-trip (partial, fits) ─────────────────────────────────────────────

  it("partial round-trip short: gap before second src", () => {
    // src → r1 → dst → rb1 → ··· → src  (6 segs ≤ 7)
    const { segs, didTruncate } = buildPathSegs(
      ["r1"],
      ["rb1"],
      "A",
      "B",
      false,
    );
    expect(texts(segs)).toEqual(["A", "r1", "B", "rb1", "···", "A"]);
    expect(dimAt(segs)).toEqual([4]); // gap before second A
    expect(didTruncate).toBe(false);
  });

  it("partial round-trip no relays: src → dst → ··· → src", () => {
    const { segs } = buildPathSegs([], ["rb1"], "A", "B", false);
    expect(texts(segs)).toEqual(["A", "B", "rb1", "···", "A"]);
  });

  // ── Round-trip truncated ───────────────────────────────────────────────────

  it("partial round-trip truncated: src → fwd1 → ··· → dst → ··· → src", () => {
    // 3 fwd + 3 ret relays: canonical = 1+3+1+3+1+1 = 10 segs, must truncate
    const { segs, didTruncate } = buildPathSegs(
      ["r1", "r2", "r3"],
      ["rb1", "rb2", "rb3"],
      "A",
      "B",
      false,
    );
    expect(texts(segs)).toEqual(["A", "r1", "···", "B", "···", "A"]);
    expect(dimAt(segs)).toEqual([2, 4]);
    expect(didTruncate).toBe(true);
  });

  it("round-trip truncated with no fwd relays: src → ··· → dst → ··· → src", () => {
    const { segs } = buildPathSegs(
      [],
      ["rb1", "rb2", "rb3", "rb4", "rb5"],
      "A",
      "B",
      false,
    );
    expect(texts(segs)).toEqual(["A", "···", "B", "···", "A"]);
  });

  // ── Complete sessions ──────────────────────────────────────────────────────

  it("complete short: no gap, full path", () => {
    const { segs, didTruncate } = buildPathSegs(
      ["r1"],
      ["rb1"],
      "A",
      "B",
      true,
    );
    expect(texts(segs)).toEqual(["A", "r1", "B", "rb1", "A"]);
    expect(dimAt(segs)).toEqual([]); // no gap indicators
    expect(didTruncate).toBe(false);
  });

  it("complete middle-truncated: shows first 2 and last 2", () => {
    // 4 fwd + 4 ret relays: parts = [A, r1,r2,r3,r4, B, rb1,rb2,rb3,rb4, A] = 11
    const { segs, didTruncate } = buildPathSegs(
      ["r1", "r2", "r3", "r4"],
      ["rb1", "rb2", "rb3", "rb4"],
      "A",
      "B",
      true,
    );
    expect(texts(segs)).toEqual(["A", "r1", "···", "rb4", "A"]);
    expect(didTruncate).toBe(true);
  });

  // ── buildFullText ─────────────────────────────────────────────────────────

  it("fullText includes all nodes without gap indicator", () => {
    const text = buildFullText(["r1", "r2"], ["rb1"], "A", "B");
    expect(text).toBe("A → r1 → r2 → B → rb1 → A");
  });

  it("fullText request-only has no trailing src", () => {
    const text = buildFullText(["r1"], [], "A", "B");
    expect(text).toBe("A → r1 → B");
  });

  // ── GAP sentinel is dim ───────────────────────────────────────────────────

  it("GAP constant is marked dim", () => {
    expect(GAP.dim).toBe(true);
    expect(GAP.text).toBe("···");
  });
});
