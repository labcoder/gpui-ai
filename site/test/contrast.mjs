// The audit the theme matrix runs, as a string to evaluate in the page.
//
// It has to be one expression sent over CDP rather than a module the page
// imports, so it is built here and shipped whole. Keeping it out of the test
// file is what makes it readable.
//
// The method is deliberately not screenshots. Forty-five themes across three
// pages is a hundred and thirty-five captures that only a human or a pixel
// baseline can judge; reading the resolved colours out of the page answers the
// question that actually matters — can this be read — in a few milliseconds per
// theme, with a number attached.

/**
 * Builds the expression that audits one page across every theme.
 *
 * The attribute is set directly rather than through the picker on purpose. The
 * picker paints through `theme.ts`, which adds the class the 200 ms cross-fade
 * runs under, and `getComputedStyle` would then return colours caught halfway
 * between two themes. Writing the attribute skips all of it, so every reading
 * is final on the same tick.
 *
 * It goes on `<body>`, not on the document element, because the site writes the
 * document element itself: any React render that reaches `paint()` puts the
 * page's own theme back, and an audit racing it measures whatever it happens to
 * catch. That is not hypothetical — it is how this audit came to pass locally
 * and fail in CI on the same commit. `themes.css` keys on `[data-theme]` at any
 * level and the properties inherit, so body is as good a place to set it, and
 * nothing in the app touches it there.
 *
 * The one thing body cannot carry is `html { font-size: var(--ai-font-size) }`,
 * so a theme with a different base size is measured at the default one. Font
 * size only decides which WCAG threshold applies, and every size on the site is
 * far below the large-text boundary either way.
 *
 * Transitions are switched off for the duration, and that is not tidiness. The
 * site cross-fades colour over two hundred milliseconds, and `getComputedStyle`
 * reports the value a transition is currently at — so forty-five themes read
 * inside one frame all return the same colour part-way between two of them, and
 * the audit finds nothing wrong with any of them. That is exactly how this gate
 * passed locally and failed in CI on the same commit.
 */
export function auditExpression(slugs) {
  return `(() => {
    const slugs = ${JSON.stringify(slugs)};

    const channel = (value) => {
      const c = value / 255;
      return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
    };
    const luminance = ([r, g, b]) =>
      0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
    const parse = (value) => {
      const parts = value.match(/[\\d.]+/g);
      if (!parts || parts.length < 3) return null;
      const rgb = parts.slice(0, 3).map(Number);
      const alpha = parts.length > 3 ? Number(parts[3]) : 1;
      return { rgb, alpha };
    };
    const ratio = (a, b) => {
      const [light, dark] = luminance(a) >= luminance(b) ? [a, b] : [b, a];
      return (luminance(light) + 0.05) / (luminance(dark) + 0.05);
    };

    // The colour actually behind an element: walk up until something paints.
    const backdrop = (element) => {
      for (let node = element; node; node = node.parentElement) {
        const parsed = parse(getComputedStyle(node).backgroundColor);
        if (parsed && parsed.alpha > 0) return parsed.rgb;
      }
      return parse(getComputedStyle(document.body).backgroundColor)?.rgb ?? [255, 255, 255];
    };

    const shown = (element) => {
      if (element.closest('[aria-hidden="true"], [hidden], [inert]')) return false;
      const style = getComputedStyle(element);
      if (style.visibility === "hidden" || style.display === "none") return false;
      const box = element.getBoundingClientRect();
      return box.width > 0 && box.height > 0;
    };

    // Text nodes, not elements: an element whose only text lives in a child has
    // that child's colour, and counting it twice invents a pair nobody sees.
    const targets = [];
    for (const element of document.querySelectorAll("body *")) {
      const hasOwnText = [...element.childNodes].some(
        (node) => node.nodeType === 3 && node.textContent.trim().length > 0,
      );
      if (!hasOwnText || !shown(element)) continue;
      targets.push(element);
    }

    // Interactive borders are a contrast requirement of their own: a control
    // whose edge nobody can see is a control nobody can find.
    const edges = [...document.querySelectorAll("input, select, button, textarea")].filter(shown);

    const describe = (element) => {
      const tag = element.tagName.toLowerCase();
      const className = typeof element.className === "string" ? element.className.trim() : "";
      return className ? tag + "." + className.split(/\\s+/).join(".") : tag;
    };

    // Every reading has to be the theme's own value, not a frame of a
    // cross-fade between two of them.
    const frozen = document.createElement("style");
    frozen.textContent =
      "*,*::before,*::after{transition:none !important;animation:none !important}";
    document.head.append(frozen);

    const findings = [];
    const previous = document.body.dataset.theme;
    // Proof the loop is doing anything at all: forty-five themes paint
    // forty-five different backgrounds, and one distinct value means the
    // attribute is being written and ignored.
    const palettes = new Set();

    for (const slug of slugs) {
      document.body.dataset.theme = slug;
      palettes.add(getComputedStyle(document.body).backgroundColor);
      const seen = new Set();

      for (const element of targets) {
        const style = getComputedStyle(element);
        const colour = parse(style.color);
        if (!colour || colour.alpha === 0) continue;

        const behind = backdrop(element);
        const size = Number.parseFloat(style.fontSize);
        const weight = Number(style.fontWeight) || 400;
        // WCAG's "large text": 24px, or 18.66px when bold.
        const large = size >= 24 || (size >= 18.66 && weight >= 700);
        const required = large ? 3 : 4.5;
        const measured = ratio(colour.rgb, behind);
        if (measured >= required) continue;

        const where = describe(element);
        const key = where + "|" + style.color + "|" + behind.join(",");
        if (seen.has(key)) continue;
        seen.add(key);
        findings.push({
          theme: slug,
          kind: "text",
          where,
          size,
          required,
          ratio: Math.round(measured * 100) / 100,
          colour: style.color,
          behind: "rgb(" + behind.join(", ") + ")",
        });
      }

      for (const element of edges) {
        const style = getComputedStyle(element);
        if (Number.parseFloat(style.borderTopWidth) === 0) continue;
        const border = parse(style.borderTopColor);
        if (!border || border.alpha === 0) continue;

        // Against what surrounds the control, not what fills it: the edge is
        // what separates one from the other.
        const behind = backdrop(element.parentElement ?? document.body);
        const measured = ratio(border.rgb, behind);
        if (measured >= 3) continue;

        const where = describe(element);
        if (seen.has("edge|" + where)) continue;
        seen.add("edge|" + where);
        findings.push({
          theme: slug,
          kind: "edge",
          where,
          required: 3,
          ratio: Math.round(measured * 100) / 100,
          colour: style.borderTopColor,
          behind: "rgb(" + behind.join(", ") + ")",
        });
      }
    }

    if (previous) document.body.dataset.theme = previous;
    else document.body.removeAttribute("data-theme");
    frozen.remove();

    return { findings, elements: targets.length, controls: edges.length, palettes: palettes.size };
  })()`;
}

/** One line per finding, short enough that forty of them are still readable. */
export function report(findings) {
  return findings
    .map(
      (finding) =>
        `${finding.theme}: ${finding.where} ${finding.ratio}:1 ` +
        `(needs ${finding.required}) ${finding.colour} on ${finding.behind}`,
    )
    .join("\n");
}
