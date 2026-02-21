export type Seg = { text: string; dim?: boolean };

export const GAP: Seg = { text: "···", dim: true };

// Maximum number of path nodes to render before truncating.
export const PATH_MAX_VISIBLE = 7;

/**
 * Build the segment list for a traceroute path display.
 *
 * Rules:
 *  - complete sessions: show full path, middle-truncate if too long.
 *  - partial/observed round-trip (resHops present): ··· goes BEFORE the
 *    second srcLabel (return-path side).
 *  - partial/observed request-only (no resHops): ··· goes AFTER the last
 *    known relay (our node) and BEFORE dst. When the relay list itself is
 *    too long, keep the first and last relays visible and insert a leading
 *    ··· between them so our node always appears just before the gap.
 *
 * Returns segs and a flag indicating whether any relay nodes were hidden
 * (used to decide whether to show a hover tooltip with the full path).
 */
export function buildPathSegs(
  reqHops: string[],
  resHops: string[],
  srcLabel: string,
  dstLabel: string,
  isComplete: boolean,
  maxVisible: number = PATH_MAX_VISIBLE,
): { segs: Seg[]; didTruncate: boolean } {
  // Budget for request-only paths: src + N relays + ··· + dst = maxVisible
  const MAX_RELAYS = maxVisible - 3;

  let segs: Seg[];
  let didTruncate = false;

  if (isComplete) {
    // Complete: show full round-trip, middle-truncate only if too long.
    const parts = [srcLabel, ...reqHops, dstLabel];
    if (resHops.length > 0) parts.push(...resHops, srcLabel);

    if (parts.length <= maxVisible) {
      segs = parts.map((t) => ({ text: t }));
    } else {
      didTruncate = true;
      segs = [
        { text: parts[0] },
        { text: parts[1] },
        GAP,
        { text: parts[parts.length - 2] },
        { text: parts[parts.length - 1] },
      ];
    }
  } else if (resHops.length > 0) {
    // Partial round-trip: ··· goes BEFORE the second src (return-path side).
    // Canonical: src → [reqHops] → dst → [resHops] → ··· → src
    const canonical: Seg[] = [
      { text: srcLabel },
      ...reqHops.map((h) => ({ text: h })),
      { text: dstLabel },
      ...resHops.map((h) => ({ text: h })),
      GAP,
      { text: srcLabel },
    ];
    if (canonical.length <= maxVisible) {
      segs = canonical;
    } else {
      // Truncated: keep src, first fwd relay, ···, dst, ···, src so both
      // endpoints and the pivot are always visible.
      didTruncate = true;
      segs = [
        { text: srcLabel },
        ...(reqHops.length > 0 ? [{ text: reqHops[0] }] : []),
        GAP,
        { text: dstLabel },
        GAP,
        { text: srcLabel },
      ];
    }
  } else {
    // Partial/observed request-only: ··· goes AFTER the last known relay
    // (our node) and BEFORE dst, signalling unknown hops beyond our vantage.
    if (reqHops.length <= MAX_RELAYS) {
      // All relays fit: src → [all relays] → ··· → dst
      segs = [
        { text: srcLabel },
        ...reqHops.map((h) => ({ text: h })),
        GAP,
        { text: dstLabel },
      ];
    } else {
      // Too many relays: show first relay for context, gap, last relay (our
      // node), gap, dst — our node is always the rightmost visible relay.
      didTruncate = true;
      segs = [
        { text: srcLabel },
        { text: reqHops[0] },
        GAP,
        { text: reqHops[reqHops.length - 1] },
        GAP,
        { text: dstLabel },
      ];
    }
  }

  return { segs, didTruncate };
}

/** Full known path as a plain string, used for hover tooltip. */
export function buildFullText(
  reqHops: string[],
  resHops: string[],
  srcLabel: string,
  dstLabel: string,
): string {
  const parts = [srcLabel, ...reqHops, dstLabel];
  if (resHops.length > 0) parts.push(...resHops, srcLabel);
  return parts.join(" → ");
}
