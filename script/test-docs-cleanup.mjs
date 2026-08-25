import { stripDeadReadMore } from './docs-cleanup.mjs';

function check(condition, message) {
  if (condition) return;
  process.stderr.write(`${message}\n`);
  process.exit(1);
}

// The shape rustdoc emits when the trait it would link to was never documented,
// straight out of target/doc/gpui_ai/orbs/struct.Orbs.html.
const dead =
  `<div class='docblock'>Sets the width of the element. ` +
  `<a href="https://tailwindcss.com/docs/width">Docs</a> <a>Read more</a></div>`;
const cleaned = stripDeadReadMore(dead);
check(cleaned.removed === 1, `expected one removal, got ${cleaned.removed}`);
check(!cleaned.html.includes('Read more'), `the dead link survived: ${cleaned.html}`);
check(
  cleaned.html ===
    `<div class='docblock'>Sets the width of the element. ` +
      `<a href="https://tailwindcss.com/docs/width">Docs</a></div>`,
  `the sentence around it was damaged: ${cleaned.html}`,
);

// A link that resolves is a link, and every one of them is left alone.
const alive =
  `<a href="https://doc.rust-lang.org/1.97.1/core/default/trait.Default.html#tymethod.default">Read more</a>`;
const untouched = stripDeadReadMore(alive);
check(untouched.removed === 0, 'a working "Read more" was removed');
check(untouched.html === alive, 'a working "Read more" was rewritten');

// Several on one page, which is the normal case: the Styled methods run to
// hundreds per type.
const many = stripDeadReadMore(`${dead}${dead}${alive}${dead}`);
check(many.removed === 3, `expected three removals, got ${many.removed}`);
check(many.html.includes(alive), 'the working link was lost among the dead ones');
check(
  (many.html.match(/Read more/g) ?? []).length === 1,
  `only the working link should be left: ${many.html}`,
);

// Nothing to do is not a rewrite: the tree is 543 files and only 156 have any.
const none = '<p>No links here at all.</p>';
check(stripDeadReadMore(none).html === none, 'an untouched page was rewritten');
check(stripDeadReadMore(none).removed === 0, 'an untouched page reported removals');

process.stdout.write('docs-cleanup: dead "Read more" anchors go, working ones stay\n');
