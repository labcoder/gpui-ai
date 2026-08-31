// Finding a component by what you remember about it.
//
// The catalog is 34 entries, so this is not about speed — it is about the fact
// that a substring match answers the wrong question. Typing "approve" matched
// nothing, because the component is called Approval card; "on_approve" matched
// nothing, because the events were never looked at; and "table" listed the
// four tables in whatever order the catalog happened to be in, with Diff table
// no more likely to be first than any other.
//
// So: index the fields a reader could plausibly remember — the name, the type,
// the category, the summary, the event names, and the behaviour notes — score
// each one by how much a hit there means, and rank. A hit on a type name is
// worth six times a hit somewhere in the prose, because someone typing
// `ToolCall` knows exactly what they want.
//
// Every term has to match something. Two words narrow rather than widen: a
// reader typing "table filter" means the one component that is both, not the
// eleven that are either.
//
// Plain JavaScript so the ranking is covered by a real test rather than only by
// whatever the page happens to show.

/**
 * What a hit in each field is worth.
 *
 * The gaps are wide on purpose. A field's score is not a weight to be tuned —
 * it is a statement about how specific a match there is, and the ordering
 * between them is the only part that has to hold.
 */
const FIELDS = [
  { name: "api", weight: 120 },
  { name: "title", weight: 100 },
  { name: "compactLabel", weight: 90 },
  { name: "events", weight: 70 },
  { name: "category", weight: 50 },
  { name: "summary", weight: 30 },
  { name: "usage", weight: 25 },
  { name: "prose", weight: 20 },
];

/** A hit at the start of a word beats one buried inside it. */
const WHOLE_WORD = 2;

/**
 * The searchable text of one record, by field.
 *
 * Built once per record rather than per keystroke: the strings never change
 * and lowercasing fifty of them on every character typed is work for nothing.
 *
 * A record is a component or a decoration. A decoration has no type name and
 * no events; those fields come back empty and score nothing, which is what
 * should happen — someone typing `ToolCall` does not mean the halftone.
 */
function haystack(record) {
  return {
    api: record.api,
    title: record.title,
    compactLabel: record.compactLabel,
    events: (record.events ?? []).join(" "),
    category: record.category,
    summary: record.summary,
    usage: record.usage,
    prose: Object.values(record.behavior ?? {}).join(" "),
  };
}

/**
 * Builds the index the page searches.
 *
 * Returned rather than held in a module variable so a test can index a handful
 * of made-up components without the real catalog being involved.
 */
export function buildIndex(records) {
  return records.map((record) => {
    const fields = haystack(record);
    return {
      component: record,
      fields: Object.fromEntries(
        Object.entries(fields).map(([name, text]) => [name, String(text ?? "").toLowerCase()]),
      ),
    };
  });
}

/** What one term is worth against one component, or 0 if it is not there. */
function scoreTerm(entry, term) {
  let best = 0;
  for (const { name, weight } of FIELDS) {
    const text = entry.fields[name];
    if (!text) continue;
    const at = text.indexOf(term);
    if (at < 0) continue;
    // At the start of a word, which is what someone typing a prefix means.
    // `\W` would treat an underscore as part of a word, and every event name
    // here is snake_case — `on_approve` has to be findable by `approve`.
    const boundary = at === 0 || /[^a-z0-9]/.test(text[at - 1]);
    best = Math.max(best, boundary ? weight * WHOLE_WORD : weight);
  }
  return best;
}

/**
 * The components matching a query, best first.
 *
 * An empty query matches everything, in catalog order: the search box narrows
 * a page that is already complete rather than being the only way to see it.
 */
export function search(index, query) {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return index.map((entry) => entry.component);

  const scored = [];
  for (const entry of index) {
    let total = 0;
    for (const term of terms) {
      const score = scoreTerm(entry, term);
      // Every term has to land somewhere. Adding a word must never widen the
      // result, or a reader narrowing a search watches it get longer.
      if (score === 0) {
        total = 0;
        break;
      }
      total += score;
    }
    if (total > 0) scored.push({ component: entry.component, total });
  }

  // Catalog order breaks ties, so equally good matches stay in the order the
  // rest of the site lists them in rather than an order nobody can predict.
  return scored
    .sort((a, b) => b.total - a.total || a.component.sequence - b.component.sequence)
    .map((hit) => hit.component);
}
