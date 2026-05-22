# Notesmith Icon Concepts - Round 3 (GPT-5.5)

## 1. Foldmark

### Description
A square app icon built from three crisp origami-like planes forming an abstract folded markdown page. The silhouette reads as a compact, angular monogram: a deep charcoal base square, a warm ivory folded sheet, a teal inner fold, and a coral triangular tuck. The negative space suggests both a document corner and a routed note being folded into place. All edges are straight and geometric, with no stroke effects, shadows, or gradients. The shape is intentionally asymmetrical, giving it a crafted but precise desktop-tool feel.

### Color Palette
- `#141821` - near-black background
- `#F4EFE3` - parchment ivory
- `#25B7A0` - technical teal
- `#E86F51` - routing coral
- `#2C3444` - muted slate fold

### Symbolism
Origami suggests markdown as raw material that can be folded, routed, and shaped without losing its simple plain-text nature. The tucked triangular fold represents captured notes being transformed into organized knowledge.

### Small Size Strategy
At 16x16, simplify to the ivory main polygon plus one teal diagonal fold and one coral corner. At 32x32, retain the slate underside and the white page silhouette. The icon relies on broad triangular masses rather than thin detail, so it remains readable at tiny sizes.

### SVG Code
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-labelledby="title desc">
  <title id="title">Foldmark Notesmith icon concept</title>
  <desc id="desc">Flat origami-inspired folded markdown page icon in ivory, teal, coral, and slate on charcoal.</desc>
  <rect width="512" height="512" rx="96" fill="#141821"/>
  <path d="M132 116h230l34 126-146 154H132V116z" fill="#F4EFE3"/>
  <path d="M362 116l34 126-104-44-42 198-28-176 140-104z" fill="#2C3444"/>
  <path d="M222 220l174 22-146 154-28-176z" fill="#25B7A0"/>
  <path d="M132 116l90 104-90 52V116z" fill="#E86F51"/>
  <path d="M132 272l90-52 28 176H132V272z" fill="#F4EFE3"/>
</svg>
```

## 2. Margin Sonata

### Description
A refined musical-notation inspired mark: a vertical manuscript staff becomes a markdown note rail, with five horizontal bars interrupted by compact editorial blocks. A single abstract notehead is drawn as a filled circle that docks into the staff, while a squared stem becomes a cursor-like writing mark. The composition sits on a dark green-black field with ivory staff lines, a blue notehead, and a restrained amber accent. The geometry is flat, balanced, and more editorial than musical, avoiding literal clefs or decorative flourishes.

### Color Palette
- `#101715` - deep green-black background
- `#EDE7D2` - manuscript ivory
- `#4E9BE6` - calm blue
- `#D99A32` - annotation amber
- `#34423F` - muted field green

### Symbolism
The staff represents structured notes over time: daily notes, tasks, and backlinks arranged into rhythm. The notehead and cursor-stem suggest writing as a composed, deliberate act rather than a pile of files.

### Small Size Strategy
At 16x16, render only three staff bars, the blue notehead, and the amber cursor stem. At 32x32, use all five bars and the left margin rail. The icon avoids tiny notation details, so it should stay recognizable as organized lines plus one strong focal mark.

### SVG Code
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-labelledby="title desc">
  <title id="title">Margin Sonata Notesmith icon concept</title>
  <desc id="desc">Flat icon with manuscript staff lines, a note-like dot, and cursor stem on a dark background.</desc>
  <rect width="512" height="512" rx="96" fill="#101715"/>
  <rect x="112" y="108" width="36" height="296" rx="18" fill="#34423F"/>
  <rect x="148" y="134" width="244" height="18" rx="9" fill="#EDE7D2"/>
  <rect x="148" y="188" width="244" height="18" rx="9" fill="#EDE7D2"/>
  <rect x="148" y="242" width="244" height="18" rx="9" fill="#EDE7D2"/>
  <rect x="148" y="296" width="244" height="18" rx="9" fill="#EDE7D2"/>
  <rect x="148" y="350" width="244" height="18" rx="9" fill="#EDE7D2"/>
  <circle cx="238" cy="242" r="54" fill="#4E9BE6"/>
  <rect x="292" y="120" width="28" height="176" rx="14" fill="#D99A32"/>
  <rect x="202" y="220" width="72" height="44" rx="10" fill="#101715"/>
  <rect x="332" y="188" width="60" height="18" rx="9" fill="#101715"/>
  <rect x="332" y="296" width="60" height="18" rx="9" fill="#101715"/>
</svg>
```

## 3. Delta Index

### Description
A river-delta icon made from bold branching channels that divide and rejoin across a dark square. The central shape is a flat ivory trunk entering from the top, splitting into teal and blue distributaries, then resolving into a row of small rectangular archive blocks at the bottom. It feels like captured input becoming routed, indexed, and deposited into organized folders. The visual language is more cartographic and natural-system inspired than network-like: no nodes, no graph dots, just flowing wedges and terminal blocks.

### Color Palette
- `#12151C` - dark ink background
- `#F1EBD8` - note ivory
- `#2BB3A3` - teal channel
- `#5D8FE8` - blue channel
- `#D75F4A` - priority red-orange
- `#293141` - archive slate

### Symbolism
A delta represents one captured stream of thought fanning into many useful destinations: inbox, projects, tasks, customers, and daily notes. The archive blocks show that flow becoming stable structure.

### Small Size Strategy
At 16x16, collapse the branches into a Y-shaped trunk plus three bottom blocks. At 32x32, preserve the five distributary shapes. The design uses thick channels with clear spacing, avoiding fine linework that would disappear.

### SVG Code
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-labelledby="title desc">
  <title id="title">Delta Index Notesmith icon concept</title>
  <desc id="desc">Flat river-delta inspired routing icon with bold branching channels and archive blocks.</desc>
  <rect width="512" height="512" rx="96" fill="#12151C"/>
  <path d="M232 92h48v112l-48 48V92z" fill="#F1EBD8"/>
  <path d="M256 188l52 52-92 92-44-44 84-100z" fill="#2BB3A3"/>
  <path d="M274 204l86 54-34 60-70-52 18-62z" fill="#5D8FE8"/>
  <path d="M238 224l-88 62 28 62 78-82-18-42z" fill="#F1EBD8"/>
  <path d="M216 332l40-66 44 66v54h-84v-54z" fill="#D75F4A"/>
  <rect x="112" y="390" width="64" height="42" rx="10" fill="#293141"/>
  <rect x="190" y="390" width="64" height="42" rx="10" fill="#2BB3A3"/>
  <rect x="268" y="390" width="64" height="42" rx="10" fill="#5D8FE8"/>
  <rect x="346" y="390" width="54" height="42" rx="10" fill="#293141"/>
</svg>
```

## 4. Counterform N

### Description
A typographic ligature concept built around a custom blocky lowercase-n / folded-N counterform. The icon is a single confident glyph-like shape: two vertical ivory stems connected by a diagonal teal join, with a red-orange internal counter carved from the left stroke and a slate notch on the right. It avoids literal notebook imagery and instead treats Notesmith as a premium writing instrument with a recognizable typographic mark. The proportions are tuned for an app icon: heavy, quiet, and instantly legible.

### Color Palette
- `#15161B` - blackened charcoal background
- `#EFE9DA` - warm white glyph
- `#25A9A1` - teal ligature join
- `#E45F45` - active capture counter
- `#3A4050` - graphite notch

### Symbolism
The ligature represents linking: separate notes joining into one coherent system through backlinks and routing. The carved counterform stands for the useful negative space in a knowledge base, where context appears through relationships.

### Small Size Strategy
At 16x16, reduce to the ivory stems and teal diagonal, with the red counter omitted if necessary. At 32x32, retain the red internal cutout and slate notch. The mark is mostly one large silhouette, making it very strong in dock, taskbar, and menu contexts.

### SVG Code
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-labelledby="title desc">
  <title id="title">Counterform N Notesmith icon concept</title>
  <desc id="desc">Flat typographic ligature mark with ivory stems, teal diagonal join, and red counterform.</desc>
  <rect width="512" height="512" rx="96" fill="#15161B"/>
  <path d="M128 392V120h84v150l118-150h72v272h-84V244L202 392h-74z" fill="#EFE9DA"/>
  <path d="M212 270l118-150h72L202 392h-74l84-122z" fill="#25A9A1"/>
  <path d="M150 144h40v102l-40 52V144z" fill="#E45F45"/>
  <path d="M318 244l84-106v254h-84V244z" fill="#3A4050"/>
  <path d="M202 392l116-148v148H202z" fill="#EFE9DA"/>
</svg>
```

## 5. Automata Ledger

### Description
A cellular-automata inspired grid icon: a compact 7-by-7 matrix of rounded square cells, selectively filled to form a diagonal propagation pattern that resembles an evolving note system. The mark sits on a dark field, with ivory inactive cells, teal active cells, blue linked cells, and a single coral captured cell. It feels computational, modular, and technical without resembling a node graph or circuit board. The cells are large enough to read as a premium grid rather than a pixel-art gimmick.

### Color Palette
- `#11161D` - dark graphite background
- `#EAE3D1` - quiet ivory cells
- `#2AB7A9` - active teal cells
- `#527FE0` - backlink blue cells
- `#E0664C` - captured coral cell
- `#252D3A` - dormant slate cells

### Symbolism
Cellular automata represent simple markdown notes producing emergent structure: tasks, backlinks, daily patterns, and routes appear from small local rules. The single coral cell is an incoming capture that changes the surrounding system.

### Small Size Strategy
At 16x16, render as a 5-by-5 simplified grid with only the teal diagonal and coral origin. At 32x32, keep the full 7-by-7 pattern but increase spacing slightly. The concept depends on block placement, so it remains legible even when individual rounded corners disappear.

### SVG Code
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512" role="img" aria-labelledby="title desc">
  <title id="title">Automata Ledger Notesmith icon concept</title>
  <desc id="desc">Flat cellular automata grid icon with ivory, teal, blue, coral, and slate cells.</desc>
  <rect width="512" height="512" rx="96" fill="#11161D"/>
  <rect x="122" y="122" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="176" y="122" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="230" y="122" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="284" y="122" width="38" height="38" rx="9" fill="#527FE0"/>
  <rect x="338" y="122" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="122" y="176" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="176" y="176" width="38" height="38" rx="9" fill="#2AB7A9"/>
  <rect x="230" y="176" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="284" y="176" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="338" y="176" width="38" height="38" rx="9" fill="#527FE0"/>
  <rect x="122" y="230" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="176" y="230" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="230" y="230" width="38" height="38" rx="9" fill="#E0664C"/>
  <rect x="284" y="230" width="38" height="38" rx="9" fill="#2AB7A9"/>
  <rect x="338" y="230" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="122" y="284" width="38" height="38" rx="9" fill="#527FE0"/>
  <rect x="176" y="284" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="230" y="284" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="284" y="284" width="38" height="38" rx="9" fill="#2AB7A9"/>
  <rect x="338" y="284" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="122" y="338" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="176" y="338" width="38" height="38" rx="9" fill="#527FE0"/>
  <rect x="230" y="338" width="38" height="38" rx="9" fill="#252D3A"/>
  <rect x="284" y="338" width="38" height="38" rx="9" fill="#EAE3D1"/>
  <rect x="338" y="338" width="38" height="38" rx="9" fill="#2AB7A9"/>
</svg>
```
