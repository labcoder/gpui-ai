# Web gallery fonts

GPUI's web platform builds its text system on an empty font database
(`CosmicTextSystem::new_without_system_fonts("IBM Plex Sans")` in
`gpui-pre-web`), and a browser has no system fonts to fall back on. A canvas
cannot see the page's `@font-face` rules either — the only way in is
`TextSystem::add_fonts` with the bytes. Without these files the demos draw
their furniture and no glyphs at all.

Four faces, not eight. Our 55 themes set no bold or italic syntax styles, so
the semibold-italic and the three remaining Lilex faces would be 800 KB of
binary that nothing ever asks for. These are the faces the gallery names:
`IBM Plex Sans` for the interface and `Lilex` for code.

## IBMPlexSans-{Regular,SemiBold,Italic}.ttf

Copyright © 2017 IBM Corp. with Reserved Font Name "Plex", under the SIL Open
Font License 1.1 — see `IBMPlexSans-LICENSE.txt`. Upstream:
<https://github.com/IBM/plex>.

## Lilex-Regular.ttf

Copyright 2019 The Lilex Project Authors, under the SIL Open Font License 1.1
— see `Lilex-LICENSE.txt`. Upstream: <https://github.com/mishamyrt/Lilex>.

Both are the copies GPUI itself shipped up to `gpui-pre` 0.3.2, so the web
gallery renders in the same faces it always did.
