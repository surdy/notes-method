# Notesmith Icon Concepts - GPT-5.5

Five flat, production-oriented icon directions for Notesmith, a professional desktop markdown notes application for technical power users. Each SVG uses a 512 x 512 viewBox, flat colors only, and a simple silhouette intended to survive favicon and dock icon scaling.

## 1. Forge Mark

### Description

A compact forge-anvil monogram built from a dark rounded-square field, a clean steel anvil silhouette, and a folded markdown note rising from it. The note uses a simple folded corner and three horizontal cuts to signal structured writing. A small copper spark sits above the anvil as a precise accent, giving the mark a crafted feel without becoming decorative or playful.

The composition is centered and heavy at the base, making it feel stable and tool-like. The anvil is simplified into straight planes and softened outer corners so it remains legible as an app icon rather than an illustration.

### Color Palette

- Primary background: `#111827`
- Steel foreground: `#E5E7EB`
- Muted graphite: `#374151`
- Forge copper accent: `#D97706`
- Deep cut color: `#0B1120`

### Symbolism

The anvil represents the "smith" in Notesmith: notes are not just captured, they are worked into durable knowledge. The folded page anchors the metaphor back to markdown notes, while the single copper spark suggests transformation, routing, and active thought.

### Small Size Strategy

At 32 x 32, keep the rounded square, anvil block, note silhouette, and copper spark. Remove the three note lines and folded-corner diagonal if needed. At 16 x 16, reduce the icon to a dark square with a bright anvil-note silhouette and one copper pixel or small square above it.

### SVG Code

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title desc">
  <title id="title">Notesmith Forge Mark Icon</title>
  <desc id="desc">Flat icon showing a note forged above a simplified anvil on a dark rounded-square background.</desc>
  <rect width="512" height="512" rx="104" fill="#111827"/>
  <path d="M152 344h208c14 0 26 12 26 26v22H126v-22c0-14 12-26 26-26Z" fill="#E5E7EB"/>
  <path d="M118 294h276c11 0 20 9 20 20v30H98v-30c0-11 9-20 20-20Z" fill="#9CA3AF"/>
  <path d="M156 250h168c19 0 36 9 47 24l15 20H126l15-20c11-15 28-24 47-24Z" fill="#E5E7EB"/>
  <path d="M92 294h68v50H92c-14 0-26-11-26-25s12-25 26-25Z" fill="#D1D5DB"/>
  <path d="M360 294h60c14 0 26 11 26 25s-12 25-26 25h-60v-50Z" fill="#D1D5DB"/>
  <path d="M198 104h112l68 68v94H198V104Z" fill="#F9FAFB"/>
  <path d="M310 104v68h68l-68-68Z" fill="#CBD5E1"/>
  <rect x="226" y="188" width="94" height="14" rx="7" fill="#374151"/>
  <rect x="226" y="220" width="116" height="14" rx="7" fill="#374151"/>
  <rect x="226" y="252" width="82" height="14" rx="7" fill="#374151"/>
  <path d="M256 72l13 29 31 4-23 21 6 31-27-16-27 16 6-31-23-21 31-4 13-29Z" fill="#D97706"/>
  <rect x="214" y="276" width="84" height="18" fill="#0B1120" opacity="0.45"/>
</svg>
```

## 2. Vault Ledger

### Description

A crisp vault-door circle is inset into a square app tile, with a folded note grid visible inside the door. The outer form is a dark neutral square, the vault is a cool blue disk, and the central ledger is rendered as bright paper planes divided by thin dark markdown rows. Four small radial bars on the vault door imply locking points and organization without drawing a literal safe.

The silhouette is simple: square tile, circular vault, rectangular note. This gives the icon strong recognition across platforms and clear scaling behavior.

### Color Palette

- Primary background: `#0F172A`
- Vault blue: `#2563EB`
- Vault blue dark: `#1D4ED8`
- Paper white: `#F8FAFC`
- Ink dark: `#172033`
- Brass accent: `#F59E0B`

### Symbolism

Notesmith works with vaults, and this mark makes the vault the primary metaphor. The ledger inside the vault signals that the protected object is organized markdown knowledge, not generic files. The brass center pin suggests control, trust, and durable structure.

### Small Size Strategy

At 32 x 32, keep the square, blue circle, white note rectangle, and brass center. The radial bars can merge into four short ticks. At 16 x 16, simplify to a blue circle with a white vertical note block and one dark horizontal row.

### SVG Code

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title desc">
  <title id="title">Notesmith Vault Ledger Icon</title>
  <desc id="desc">Flat icon showing a vault door containing an organized markdown ledger.</desc>
  <rect width="512" height="512" rx="96" fill="#0F172A"/>
  <circle cx="256" cy="256" r="174" fill="#2563EB"/>
  <circle cx="256" cy="256" r="138" fill="#1D4ED8"/>
  <rect x="168" y="148" width="176" height="220" rx="22" fill="#F8FAFC"/>
  <path d="M298 148v58h46l-46-58Z" fill="#CBD5E1"/>
  <rect x="196" y="208" width="120" height="16" rx="8" fill="#172033"/>
  <rect x="196" y="248" width="104" height="16" rx="8" fill="#172033"/>
  <rect x="196" y="288" width="126" height="16" rx="8" fill="#172033"/>
  <rect x="196" y="328" width="82" height="16" rx="8" fill="#172033"/>
  <circle cx="256" cy="256" r="34" fill="#F59E0B"/>
  <circle cx="256" cy="256" r="16" fill="#0F172A"/>
  <rect x="246" y="92" width="20" height="58" rx="10" fill="#93C5FD"/>
  <rect x="246" y="362" width="20" height="58" rx="10" fill="#93C5FD"/>
  <rect x="92" y="246" width="58" height="20" rx="10" fill="#93C5FD"/>
  <rect x="362" y="246" width="58" height="20" rx="10" fill="#93C5FD"/>
</svg>
```

## 3. Link Quench

### Description

Two interlocked chain links form a bold angular N over a dark circular field. The links are flat cyan and silver, with squared inner cuts so the shape feels technical and precise. A small vertical note marker sits at the lower right, acting like a cursor, bookmark, or captured task entering the linked system.

The design avoids literal paper as the main subject and instead focuses on backlinks, cross-reference, and knowledge graph behavior. It is compact, geometric, and particularly readable in menu bars or browser tabs.

### Color Palette

- Primary background: `#111827`
- Link cyan: `#22D3EE`
- Link silver: `#E2E8F0`
- Ink cut color: `#111827`
- Task accent: `#10B981`

### Symbolism

Notesmith is built around links and backlinks. The interlocked forms imply bidirectional context, connected customer notes, and daily work streams. The resulting N silhouette gives the icon a subtle brand monogram without relying on text.

### Small Size Strategy

At 32 x 32, preserve the two link masses and the central dark cuts. The task marker can become a small green square. At 16 x 16, collapse the design into a cyan-and-white angled N with one dark negative-space break.

### SVG Code

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title desc">
  <title id="title">Notesmith Link Quench Icon</title>
  <desc id="desc">Flat icon showing interlocked chain links arranged into a sharp N-like mark.</desc>
  <rect width="512" height="512" rx="104" fill="#F8FAFC"/>
  <circle cx="256" cy="256" r="190" fill="#111827"/>
  <path d="M150 318c-37-37-37-97 0-134l36-36c37-37 97-37 134 0l28 28-48 48-28-28c-10-10-26-10-36 0l-36 36c-10 10-10 26 0 36l44 44-48 48-46-42Z" fill="#22D3EE"/>
  <path d="M362 194c37 37 37 97 0 134l-36 36c-37 37-97 37-134 0l-28-28 48-48 28 28c10 10 26 10 36 0l36-36c10-10 10-26 0-36l-44-44 48-48 46 42Z" fill="#E2E8F0"/>
  <path d="M218 232l62-62 34 34-62 62-34-34Z" fill="#111827"/>
  <path d="M198 306l62-62 34 34-62 62-34-34Z" fill="#111827"/>
  <rect x="338" y="336" width="52" height="84" rx="14" fill="#10B981"/>
  <rect x="352" y="354" width="24" height="8" rx="4" fill="#ECFDF5"/>
  <rect x="352" y="374" width="24" height="8" rx="4" fill="#ECFDF5"/>
</svg>
```

## 4. Routed Page

### Description

A folded markdown page sits slightly forward on a deep ink tile. Behind and around it, three flat route lanes enter from the left and exit to organized destinations on the right. The lanes are simple orthogonal strokes with square turns, giving the icon the feel of a routing diagram or command palette without becoming a flowchart.

The note is intentionally large and centered for immediate readability. The route colors are restrained but distinct, representing capture, tasks, and customer streams.

### Color Palette

- Primary background: `#18181B`
- Paper foreground: `#F4F4F5`
- Fold color: `#A1A1AA`
- Route blue: `#38BDF8`
- Route green: `#34D399`
- Route amber: `#FBBF24`
- Ink dark: `#27272A`

### Symbolism

Notesmith helps users capture work and route it into the right place: daily notes, customer folders, tasks, and templates. This icon makes that operational strength visible. The page remains the central object, while the lanes show movement from raw input to organized output.

### Small Size Strategy

At 32 x 32, keep one page shape and three colored route strokes. The page lines can reduce to two bars. At 16 x 16, use a white page silhouette with one or two colored side ticks to imply routing.

### SVG Code

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title desc">
  <title id="title">Notesmith Routed Page Icon</title>
  <desc id="desc">Flat icon showing a markdown note with colored routing lanes around it.</desc>
  <rect width="512" height="512" rx="96" fill="#18181B"/>
  <path d="M72 164h94c20 0 36 16 36 36v4" fill="none" stroke="#38BDF8" stroke-width="34" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M72 256h88" fill="none" stroke="#34D399" stroke-width="34" stroke-linecap="round"/>
  <path d="M72 348h94c20 0 36-16 36-36v-4" fill="none" stroke="#FBBF24" stroke-width="34" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M310 204h40c20 0 36-16 36-36v-28h54" fill="none" stroke="#38BDF8" stroke-width="34" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M310 256h130" fill="none" stroke="#34D399" stroke-width="34" stroke-linecap="round"/>
  <path d="M310 308h40c20 0 36 16 36 36v28h54" fill="none" stroke="#FBBF24" stroke-width="34" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="156" y="104" width="200" height="304" rx="28" fill="#F4F4F5"/>
  <path d="M296 104v70h60l-60-70Z" fill="#A1A1AA"/>
  <rect x="196" y="194" width="118" height="18" rx="9" fill="#27272A"/>
  <rect x="196" y="242" width="92" height="18" rx="9" fill="#27272A"/>
  <rect x="196" y="290" width="126" height="18" rx="9" fill="#27272A"/>
  <rect x="196" y="338" width="76" height="18" rx="9" fill="#27272A"/>
</svg>
```

## 5. Tempered Grid

### Description

A strong diamond-shaped forge tile contains a precise knowledge grid: four note cells connected by a minimal cross-link. The diamond is slate-black, the note cells are warm white, and one copper cell is highlighted as the active note or task. Small square nodes at the joins make the system feel like an engineered workspace rather than an abstract graph.

The icon is intentionally architectural. It has the compact silhouette of a premium tool icon while communicating structure, vault organization, and relationships between notes.

### Color Palette

- Primary background: `#F8FAFC`
- Diamond field: `#1F2937`
- Note cells: `#F9FAFB`
- Active copper: `#C2410C`
- Connector teal: `#14B8A6`
- Ink cuts: `#111827`

### Symbolism

The diamond references a forged ingot or maker's stamp, while the grid represents organized vault structure: customer folders, tasks, daily notes, and templates. The teal connectors point to links and backlinks; the copper cell marks active work being shaped into the system.

### Small Size Strategy

At 32 x 32, retain the diamond, four cells, and central connector. Internal note lines can disappear. At 16 x 16, simplify to a dark diamond with four light pixels or blocks and a single copper block for the active item.

### SVG Code

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="title desc">
  <title id="title">Notesmith Tempered Grid Icon</title>
  <desc id="desc">Flat icon showing a forged diamond tile containing connected note cells.</desc>
  <rect width="512" height="512" rx="92" fill="#F8FAFC"/>
  <path d="M256 48 464 256 256 464 48 256 256 48Z" fill="#1F2937"/>
  <path d="M164 184h184v144H164V184Z" fill="none" stroke="#14B8A6" stroke-width="28" stroke-linecap="round" stroke-linejoin="round"/>
  <path d="M256 184v144" fill="none" stroke="#14B8A6" stroke-width="28" stroke-linecap="round"/>
  <path d="M164 256h184" fill="none" stroke="#14B8A6" stroke-width="28" stroke-linecap="round"/>
  <rect x="126" y="132" width="104" height="104" rx="20" fill="#F9FAFB"/>
  <rect x="282" y="132" width="104" height="104" rx="20" fill="#F9FAFB"/>
  <rect x="126" y="276" width="104" height="104" rx="20" fill="#F9FAFB"/>
  <rect x="282" y="276" width="104" height="104" rx="20" fill="#C2410C"/>
  <rect x="154" y="166" width="48" height="10" rx="5" fill="#111827"/>
  <rect x="154" y="192" width="34" height="10" rx="5" fill="#111827"/>
  <rect x="310" y="166" width="48" height="10" rx="5" fill="#111827"/>
  <rect x="310" y="192" width="34" height="10" rx="5" fill="#111827"/>
  <rect x="154" y="310" width="48" height="10" rx="5" fill="#111827"/>
  <rect x="154" y="336" width="34" height="10" rx="5" fill="#111827"/>
  <rect x="310" y="310" width="48" height="10" rx="5" fill="#FFF7ED"/>
  <rect x="310" y="336" width="34" height="10" rx="5" fill="#FFF7ED"/>
  <rect x="240" y="240" width="32" height="32" rx="8" fill="#14B8A6"/>
</svg>
```
