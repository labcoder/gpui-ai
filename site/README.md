# Showcase site

The public showcase is generated from plain HTML, CSS, and JavaScript. Its catalog metadata lives in `src/catalog.js`; `npm run build` emits stable physical routes for all 24 components and copies the already-built gallery host once into `dist/gallery`.

From the repository root:

```sh
npm run check:web
npm run build:web
```

The root build first produces the shared Vite/WebAssembly gallery, then generates the static site. Deployment is intentionally separate from the repository workflow.
