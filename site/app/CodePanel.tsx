import { useEffect, useRef, useState } from "react";
import { highlighted, snippet, type CodeToken } from "./data";

/** How long the copy result stays on screen before the button is plain again. */
const FEEDBACK_MS = 2_400;

/**
 * A snippet, highlighted at build time, with a way to take it away.
 *
 * The highlighting is token data from `site/generated/highlight.json`, rendered
 * as spans the stylesheet colours from `--ai-*` properties — so code re-skins
 * with the rest of the page, and no highlighter is shipped to the browser.
 *
 * Copy reads the plain snippet from the data layer, never the DOM. That is the
 * whole reason it cannot paste markup, spans, or a stray line number: the
 * highlighted form is not on the path.
 */
export function CodePanel({
  slug,
  variant = "default",
  label,
  actions,
}: {
  readonly slug: string;
  readonly variant?: string;
  readonly label: string;
  readonly actions?: readonly { readonly href: string; readonly text: string }[];
}) {
  const code = snippet(slug, variant);
  if (!code) return null;

  return (
    <div className="code-panel">
      <div className="code-actions">
        <CopyButton code={code} label={label} />
        {actions?.map((action) => (
          <a key={action.href} href={action.href}>
            {action.text}
          </a>
        ))}
      </div>
      <Code lines={highlighted(slug, variant)} fallback={code} />
    </div>
  );
}

/**
 * The tokens as spans, or the plain text if nothing highlighted this one.
 *
 * Exported because the home page's dependency lines are highlighted the same
 * way and have to be painted the same way; they are simply not cut from a
 * story, so they arrive as tokens rather than as a slug.
 */
export function Code({
  lines,
  fallback,
}: {
  readonly lines: readonly (readonly CodeToken[])[] | undefined;
  readonly fallback: string;
}) {
  if (!lines) {
    return (
      <pre className="code">
        <code>{fallback}</code>
      </pre>
    );
  }

  return (
    <pre className="code">
      <code>
        {lines.map((line, index) => (
          // Lines have no identity of their own; their position is what they
          // are, and the list never reorders.
          // eslint-disable-next-line react/no-array-index-key
          <span className="code-line" key={index}>
            {line.map(([text, category], token) => (
              // eslint-disable-next-line react/no-array-index-key
              <span className={category ? `t-${category}` : undefined} key={token}>
                {text}
              </span>
            ))}
            {"\n"}
          </span>
        ))}
      </code>
    </pre>
  );
}

/**
 * Copy, and say what happened.
 *
 * The clipboard can be refused — an insecure origin, a browser that wants a
 * gesture it did not see, a permission denied — and a button that silently
 * does nothing is worse than one that admits it. The status is a live region
 * so the result is announced rather than only shown.
 */
function CopyButton({ code, label }: { readonly code: string; readonly label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

  useEffect(() => () => clearTimeout(timer.current), []);

  const copy = async () => {
    clearTimeout(timer.current);
    try {
      await navigator.clipboard.writeText(code);
      setState("copied");
    } catch {
      setState("failed");
    }
    timer.current = setTimeout(() => setState("idle"), FEEDBACK_MS);
  };

  return (
    <>
      <button type="button" data-copy onClick={copy}>
        Copy
      </button>
      <span className="copy-status" role="status" aria-live="polite">
        {state === "copied" ? `Copied ${label}` : null}
        {state === "failed" ? "Your browser would not let the page use the clipboard" : null}
      </span>
    </>
  );
}
