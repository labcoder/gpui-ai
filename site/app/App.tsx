import { categories, components, hero, themes } from "./data";

/**
 * The scaffold's placeholder.
 *
 * S-08 replaces this with the real pages. It renders counts rather than
 * "hello" so the scaffold proves the generated data actually loads and is
 * typed, which is the part that could silently be wrong.
 */
export function App() {
  return (
    <main>
      <h1>gpui-ai</h1>
      <p>
        Site scaffold. {components.length} components across {categories.length} categories,{" "}
        {themes.length} themes, hero story <code>{hero.slug}</code>.
      </p>
    </main>
  );
}
