# CopperMoon Design System

> The unified design language for the CopperMoon ecosystem.
> All apps — **coppermine**, **coppermoon-dev**, **copper-blog** — share these foundations.

---

## Table of Contents

1. [Design Principles](#1-design-principles)
2. [Color System](#2-color-system)
3. [Typography](#3-typography)
4. [Spacing & Layout](#4-spacing--layout)
5. [Border Radius](#5-border-radius)
6. [Shadows & Elevation](#6-shadows--elevation)
7. [Transitions & Animations](#7-transitions--animations)
8. [Components](#8-components)
   - [Buttons](#buttons)
   - [Cards](#cards)
   - [Inputs](#inputs)
   - [Badges](#badges)
   - [Alerts & Callouts](#alerts--callouts)
   - [Code Blocks](#code-blocks)
9. [Layout Patterns](#9-layout-patterns)
   - [Navigation](#navigation)
   - [Sidebar](#sidebar)
   - [Hero Sections](#hero-sections)
   - [Content Sections](#content-sections)
   - [Footer](#footer)
10. [Responsive Design](#10-responsive-design)
11. [Glass Morphism & Effects](#11-glass-morphism--effects)
12. [Icons & Assets](#12-icons--assets)
13. [Syntax Highlighting](#13-syntax-highlighting)
14. [Theme Presets](#14-theme-presets)
15. [Implementation Reference](#15-implementation-reference)

---

## 1. Design Principles

- **Dark-first**: Pure black backgrounds with layered elevation. Light mode is a secondary preset.
- **Copper accent**: The copper palette is the single brand color — used sparingly for focus, CTAs, and interactive states.
- **Minimalism**: Inspired by Vercel and Nothing Phone aesthetics. Generous whitespace, restrained color, sharp typography.
- **Glass morphism**: Translucent surfaces with backdrop blur for depth.
- **Motion with purpose**: Scroll-triggered reveals, subtle hover lifts, and glow effects — never gratuitous.
- **Responsive by default**: Mobile-first breakpoints, fluid grids, adaptive spacing.

---

## 2. Color System

### 2.1 Brand — Copper

The primary accent palette. `copper-500` is the canonical brand color.

| Token        | Hex       | Usage                              |
|--------------|-----------|-------------------------------------|
| `copper-50`  | `#fdf8f3` | Lightest tint (light theme bg)      |
| `copper-100` | `#f9ede0` | Light tint                          |
| `copper-200` | `#f2d7bc` | Light accent                        |
| `copper-300` | `#e8ba8e` | Hover text on dark, light accent    |
| `copper-400` | `#dc955d` | Secondary accent, icon color        |
| `copper-500` | `#c97c3c` | **Primary brand color** — buttons, links, focus rings |
| `copper-600` | `#b86830` | Darker accent                       |
| `copper-700` | `#99522a` | Dark accent                         |
| `copper-800` | `#7c4428` | Very dark accent                    |
| `copper-900` | `#653923` | Deepest tone                        |
| `copper-950` | `#361c10` | Near-black copper                   |

### 2.2 Neutrals — Zinc

All UI surfaces, text, and borders use the Tailwind **zinc** scale.

| Token       | Hex       | Role                               |
|-------------|-----------|-------------------------------------|
| `zinc-50`   | `#fafafa` | Primary text (dark theme)           |
| `zinc-100`  | `#f4f4f5` | Secondary text (dark theme)         |
| `zinc-200`  | `#e4e4e7` | Headings on light theme             |
| `zinc-300`  | `#d4d4d8` | Body text (dark theme lead)         |
| `zinc-400`  | `#a1a1aa` | Muted text, placeholders            |
| `zinc-500`  | `#71717a` | Tertiary text, disabled             |
| `zinc-600`  | `#52525b` | Subtle borders (light theme)        |
| `zinc-700`  | `#3f3f46` | Interactive borders                 |
| `zinc-800`  | `#27272a` | Card borders, dividers (dark theme) |
| `zinc-900`  | `#18181b` | Card backgrounds (dark theme)       |
| `zinc-950`  | `#09090b` | Page background                     |

### 2.3 Background Layers (Dark Theme)

Elevation is communicated through progressively lighter backgrounds.

| CSS Variable       | Value     | Layer                |
|--------------------|-----------|----------------------|
| `--bg-primary`     | `#000000` | Page background      |
| `--bg-secondary`   | `#0a0a0a` | Cards, panels        |
| `--bg-tertiary`    | `#111111` | Inputs, nested cards |
| `--bg-elevated`    | `#171717` | Focused inputs, popovers |
| `--bg-hover`       | `#1a1a1a` | Hover state          |

### 2.4 Border Colors (Dark Theme)

| CSS Variable         | Value     | Usage                |
|----------------------|-----------|----------------------|
| `--border-primary`   | `#222222` | Default borders      |
| `--border-secondary` | `#333333` | Stronger dividers    |
| `--border-accent`    | `#c97c3c` | Active / focus       |

### 2.5 Text Colors (Dark Theme)

| CSS Variable       | Value     | Usage                 |
|--------------------|-----------|-----------------------|
| `--text-primary`   | `#fafafa` | Headings, body text   |
| `--text-secondary` | `#a1a1a1` | Descriptions, meta    |
| `--text-tertiary`  | `#666666` | Disabled, breadcrumbs |

### 2.6 Semantic Colors

| Semantic | Color       | Hex       | Usage              |
|----------|-------------|-----------|--------------------|
| Info     | Blue 500    | `#3b82f6` | Informational      |
| Success  | Green 500   | `#22c55e` | Positive feedback  |
| Warning  | Amber/Copper| `#c97c3c` | Caution states     |
| Danger   | Red 500     | `#ef4444` | Errors, destructive|

### 2.7 Accent Glow

```
rgba(201, 124, 60, 0.15)   — Subtle glow backgrounds
rgba(201, 124, 60, 0.25)   — Box-shadow glow (large)
rgba(201, 124, 60, 0.20)   — Box-shadow glow (small)
```

---

## 3. Typography

### 3.1 Font Families

| Role      | Primary          | Fallback                                    |
|-----------|------------------|---------------------------------------------|
| Sans      | **Geist** / **Inter** | `-apple-system, BlinkMacSystemFont, Segoe UI, sans-serif` |
| Mono      | **Geist Mono** / **JetBrains Mono** | `Fira Code, monospace`        |

> `coppermine` uses Geist; `coppermoon-dev` uses Inter. Both are interchangeable within the system.

### 3.2 Font Weights

| Weight | Value | Usage                          |
|--------|-------|--------------------------------|
| Light  | 300   | Subtle text, decorative        |
| Regular| 400   | Body text                      |
| Medium | 500   | Subheadings, buttons, nav links|
| Semibold| 600  | Section headings, labels       |
| Bold   | 700   | Main headings                  |
| Extrabold| 800 | Hero titles                    |

### 3.3 Type Scale

| Element         | Size             | Weight   | Line Height | Letter Spacing | Tailwind Class        |
|-----------------|------------------|----------|-------------|----------------|-----------------------|
| Hero h1         | `3.5rem–5rem`    | 700–800  | 0.9–1.0     | `-0.04em`      | `text-6xl lg:text-8xl`|
| Page h1         | `2.25rem`        | 700      | 1.1         | `-0.02em`      | `text-4xl lg:text-5xl`|
| Section h2      | `1.5rem–1.75rem` | 600      | 1.2         | `-0.02em`      | `text-3xl lg:text-4xl`|
| Subsection h3   | `1.125rem–1.5rem`| 600      | 1.3         | —              | `text-2xl`            |
| Card title h4   | `1rem–1.25rem`   | 500      | 1.3         | —              | `text-xl`             |
| Body            | `15px` (`1rem`)  | 400      | 1.6–1.8     | —              | `text-base`           |
| Body small      | `14px`           | 400      | 1.6         | —              | `text-sm`             |
| Caption / Label | `12px`           | 500      | 1.4         | `0.05–0.08em`  | `text-xs`             |
| Code block      | `13px`           | 400      | 1.7         | —              | `text-[13px]`         |
| Inline code     | `0.9em`          | 400      | —           | —              | —                     |

### 3.4 Heading Styles

All headings use `tracking-tight` for a modern, compact appearance.

```
h1: font-bold,     tracking-tight, text-zinc-50
h2: font-semibold, tracking-tight, text-zinc-50
h3: font-semibold, text-zinc-100
h4: font-medium,   text-zinc-200
h5: font-medium,   text-zinc-300
h6: font-medium,   text-zinc-400
```

### 3.5 Text Utilities

| Class                | Effect                                             |
|----------------------|----------------------------------------------------|
| `.text-gradient`     | `bg-clip-text text-transparent` copper-to-amber    |
| `.text-gradient-subtle`| `bg-clip-text text-transparent` zinc-100-to-400  |
| `lead`               | `text-xl text-zinc-300 leading-relaxed`            |
| `muted`              | `text-sm text-zinc-500`                            |

---

## 4. Spacing & Layout

### 4.1 Spacing Scale

Based on a **4px base unit**.

| CSS Variable  | Value  | Tailwind |
|---------------|--------|----------|
| `--space-1`   | `4px`  | `1`      |
| `--space-2`   | `8px`  | `2`      |
| `--space-3`   | `12px` | `3`      |
| `--space-4`   | `16px` | `4`      |
| `--space-5`   | `24px` | `6`      |
| `--space-6`   | `32px` | `8`      |
| `--space-8`   | `48px` | `12`     |
| `--space-10`  | `64px` | `16`     |

### 4.2 Layout Constants

| Constant             | Value   | Usage                        |
|----------------------|---------|------------------------------|
| `--header-height`    | `64px`  | Fixed header height          |
| `--sidebar-width`    | `260px` | Documentation sidebar width  |
| `--content-max-width`| `720px` | Prose content max width      |

### 4.3 Container Widths

| Width       | Tailwind     | Usage                    |
|-------------|--------------|--------------------------|
| `720px`     | `max-w-3xl`  | Prose / doc content      |
| `1024px`    | `max-w-5xl`  | Narrower pages           |
| `1152px`    | `max-w-6xl`  | Main sections            |
| `1280px`    | `max-w-7xl`  | Full-width containers    |

### 4.4 Section Padding

| Variant      | Padding              | Tailwind                |
|--------------|----------------------|-------------------------|
| Default      | `64px 0` / `96px 0`  | `py-16 lg:py-24`       |
| Small        | `32px 0` / `48px 0`  | `py-8 lg:py-12`        |
| Large        | `96px 0` / `128px 0` | `py-24 lg:py-32`       |

### 4.5 Horizontal Padding (Responsive)

```
px-4 sm:px-6 lg:px-8
```

---

## 5. Border Radius

| CSS Variable     | Value    | Tailwind       | Usage                        |
|------------------|----------|----------------|------------------------------|
| `--radius-sm`    | `4px`    | `rounded`      | Inline code, small elements  |
| `--radius-md`    | `8px`    | `rounded-lg`   | Buttons, inputs, code blocks |
| `--radius-lg`    | `12px`   | `rounded-xl`   | Cards, alerts                |
| —                | `16px`   | `rounded-2xl`  | Glass cards, large panels    |
| `--radius-full`  | `9999px` | `rounded-full` | Badges, pills, search input  |

---

## 6. Shadows & Elevation

### 6.1 Box Shadows

| Name             | Value                                          | Usage                |
|------------------|------------------------------------------------|----------------------|
| Glow copper      | `0 0 60px -12px rgba(201,124,60,0.25)`         | Hero backgrounds     |
| Glow copper sm   | `0 0 30px -8px rgba(201,124,60,0.2)`           | Card accents         |
| Elevated card    | `shadow-lg shadow-black/20`                    | Raised cards         |
| Hover lift       | `shadow-xl shadow-black/30`                    | Card hover           |
| Copper focus     | `0 0 0 3px rgba(201,124,60,0.15)`             | Input focus ring     |

### 6.2 Elevation Model (Dark Theme)

```
Level 0 — #000000  (page)
Level 1 — #0a0a0a  (cards, sidebar)
Level 2 — #111111  (inputs, nested panels)
Level 3 — #171717  (popovers, elevated)
Level 4 — #1a1a1a  (hover states)
```

No box-shadow is needed between levels — background color alone communicates depth.

---

## 7. Transitions & Animations

### 7.1 Timing

| CSS Variable          | Value       | Usage               |
|-----------------------|-------------|----------------------|
| `--transition-fast`   | `150ms ease`| Hover colors, links  |
| `--transition-base`   | `200ms ease`| General interactions |
| `--transition-slow`   | `300ms ease`| Layout shifts, cards |

### 7.2 Keyframe Animations

#### Fade Up (entrance)
```css
@keyframes fadeUp {
  from { opacity: 0; transform: translateY(20px); }
  to   { opacity: 1; transform: translateY(0); }
}
/* 0.6s ease-out, with staggered delay variants: 0.1s, 0.2s, 0.3s */
```

#### Float (decorative orbs)
```css
@keyframes float {
  0%, 100% { transform: translateY(0) rotate(0deg); }
  33%      { transform: translateY(-12px) rotate(1deg); }
  66%      { transform: translateY(6px) rotate(-1deg); }
}
/* 6s–8s ease-in-out infinite, with reverse variant */
```

#### Shimmer (gradient text)
```css
@keyframes shimmer {
  0%   { background-position: -200% center; }
  100% { background-position: 200% center; }
}
/* 3s ease-in-out infinite */
```

#### Glow Pulse
```css
@keyframes glowPulse {
  0%, 100% { opacity: 0.4; transform: scale(1); }
  50%      { opacity: 0.7; transform: scale(1.05); }
}
/* 4s ease-in-out infinite */
```

#### Border Glow
```css
@keyframes borderGlow {
  0%, 100% { border-color: rgba(201, 124, 60, 0.1); }
  50%      { border-color: rgba(201, 124, 60, 0.3); }
}
/* 3s ease-in-out infinite */
```

### 7.3 Scroll-Triggered Reveal

```css
.reveal {
  opacity: 0;
  transform: translateY(30px);
  transition: opacity 0.7s cubic-bezier(0.16, 1, 0.3, 1),
              transform 0.7s cubic-bezier(0.16, 1, 0.3, 1);
}
.reveal.visible {
  opacity: 1;
  transform: translateY(0);
}
/* .reveal-delay-1 through .reveal-delay-5: 0.1s–0.5s stagger */
```

**Scale variant:**
```css
.reveal-scale {
  opacity: 0;
  transform: scale(0.95) translateY(20px);
}
.reveal-scale.visible {
  opacity: 1;
  transform: scale(1) translateY(0);
}
```

### 7.4 Interaction Micro-Animations

| Element  | Hover Effect                             |
|----------|------------------------------------------|
| Button   | `translateY(-1px)`, opacity change       |
| Card     | `translateY(-2px)`, border glow, shadow  |
| Link     | Color transition `150ms`                 |
| Sidebar  | Chevron rotation, height collapse        |

---

## 8. Components

### Buttons

#### Variants

| Variant     | Background                | Text         | Border              | Hover                          |
|-------------|---------------------------|--------------|---------------------|--------------------------------|
| **Primary** | `copper-500`              | `black`      | —                   | `copper-400`, lift `-1px`      |
| **Secondary**| `transparent`            | `zinc-200`   | `zinc-700`          | `bg-zinc-800`, border `copper-500`|
| **Ghost**   | `transparent`             | `zinc-400`   | —                   | `text-white`, `bg-zinc-800`    |
| **Outline** | `transparent`             | `white`      | `zinc-700`          | `bg-zinc-800`, border `copper-500`|
| **Danger**  | `red-600`                 | `white`      | —                   | `red-500`                      |
| **Link**    | `transparent`             | `copper-500` | —                   | `copper-400`, underline        |

#### Sizes

| Size | Padding          | Font Size  | Tailwind              |
|------|------------------|------------|-----------------------|
| XS   | `10px 6px`       | `12px`     | `px-2.5 py-1.5 text-xs`|
| SM   | `12px 8px`       | `14px`     | `px-3 py-2 text-sm`   |
| MD   | `16px 10px`      | `14px`     | `px-4 py-2.5 text-sm` |
| LG   | `20px 12px`      | `16px`     | `px-5 py-3 text-base` |
| XL   | `24px 14px`      | `18px`     | `px-6 py-3.5 text-lg` |

#### Base Styles

```
display: inline-flex
align-items: center
gap: 8px
border-radius: 8px (rounded-lg)
font-weight: 500–600
cursor: pointer
transition: all 150ms ease
```

---

### Cards

#### Variants

| Variant         | Background           | Border                 | Hover                                  |
|-----------------|----------------------|------------------------|----------------------------------------|
| **Default**     | `zinc-900`           | `zinc-800`             | `border-zinc-700`                      |
| **Glass**       | `white/[0.03]` + blur| `white/[0.06]`         | `bg-white/[0.06]`, `border-white/[0.1]`|
| **Elevated**    | `zinc-900`           | `zinc-800`             | `shadow-xl`                            |
| **Outline**     | `transparent`        | `zinc-800`             | `border-copper-500/50`                 |
| **Ghost**       | `zinc-900/50`        | —                      | `bg-zinc-900`                          |
| **Interactive** | `zinc-900`           | `zinc-800`             | `border-copper-500`, copper shadow glow|

#### Structure

```
┌─────────────────────────────────┐
│  padding: 24px (p-6)           │
│                                 │
│  [Icon / Image]                 │
│                                 │
│  Title    — font-medium, 1rem   │
│  Desc     — text-sm, zinc-400   │
│                                 │
│  [Actions / Footer]             │
│                                 │
└─────────────────────────────────┘

Border radius: 12px (rounded-xl) or 16px (rounded-2xl) for glass
```

#### Padding Options

| Size | Value  | Tailwind |
|------|--------|----------|
| SM   | `16px` | `p-4`   |
| MD   | `24px` | `p-6`   |
| LG   | `32px` | `p-8`   |

---

### Inputs

#### Variants

| Variant     | Background      | Border        | Focus                                 |
|-------------|-----------------|---------------|---------------------------------------|
| **Default** | `zinc-900`      | `zinc-800`    | `border-copper-500`, ring `copper-500/20`|
| **Filled**  | `zinc-800`      | `transparent` | `bg-zinc-900`, `border-copper-500`    |
| **Outline** | `transparent`   | `zinc-700`    | `border-copper-500`                   |

#### Sizes

| Size | Padding         | Font  | Tailwind             |
|------|-----------------|-------|----------------------|
| SM   | `12px 6px`      | `14px`| `px-3 py-1.5 text-sm`|
| MD   | `16px 10px`     | `14px`| `px-4 py-2.5 text-sm`|
| LG   | `16px 12px`     | `16px`| `px-4 py-3 text-base`|

#### Base Styles

```
width: 100%
border-radius: 8px (rounded-lg)
color: white
placeholder: zinc-500
outline: none
transition: all 200ms ease
```

#### Search Input

```
border-radius: 9999px (rounded-full)
padding-left: 36px (icon space)
icon: positioned absolute, left 12px, color zinc-500
```

---

### Badges

#### Variants

| Variant     | Background         | Text          |
|-------------|--------------------|---------------|
| **Default** | `zinc-800`         | `zinc-300`    |
| **Primary** | `copper-500/20`    | `copper-400`  |
| **Success** | `green-500/20`     | `green-400`   |
| **Warning** | `yellow-500/20`    | `yellow-400`  |
| **Danger**  | `red-500/20`       | `red-400`     |
| **Info**    | `blue-500/20`      | `blue-400`    |

#### Sizes

| Size | Padding        | Font  | Tailwind            |
|------|----------------|-------|---------------------|
| SM   | `8px 2px`      | `12px`| `px-2 py-0.5 text-xs`|
| MD   | `10px 4px`     | `12px`| `px-2.5 py-1 text-xs`|
| LG   | `12px 4px`     | `14px`| `px-3 py-1 text-sm` |

#### Base

```
display: inline-flex
align-items: center
border-radius: 9999px (rounded-full)
font-weight: 500
```

#### Section Label Badge

Used above section headings to categorize content:

```
inline-flex items-center gap-2 px-3 py-1
rounded-full text-xs font-medium
bg-copper-500/10 text-copper-400 border border-copper-500/20
margin-bottom: 24px
```

---

### Alerts & Callouts

#### Alert Variants

| Variant     | Background        | Border             | Text          |
|-------------|-------------------|--------------------|---------------|
| **Default** | `zinc-900`        | `zinc-800`         | `zinc-300`    |
| **Info**    | `blue-500/10`     | `blue-500/20`      | `blue-400`    |
| **Success** | `green-500/10`    | `green-500/20`     | `green-400`   |
| **Warning** | `yellow-500/10`   | `yellow-500/20`    | `yellow-400`  |
| **Danger**  | `red-500/10`      | `red-500/20`       | `red-400`     |

#### Callout (Documentation)

Used in `coppermine` for doc content with a left accent border:

```
padding: 16px 24px
border-radius: 8px
margin: 24px 0
border-left: 3px solid [semantic-color]
background: var(--bg-secondary)
```

---

### Code Blocks

#### Inline Code

```
font-family: var(--font-mono)
font-size: 0.9em
background: var(--bg-tertiary) or zinc-800
padding: 2px 6px
border-radius: 4px
border: 1px solid var(--border-primary)
color: copper-400 (in doc content)
```

#### Block Code

```
background: var(--bg-secondary) or #0d1117
border: 1px solid var(--border-primary) or zinc-800/80
border-radius: 8px (rounded-xl for coppermoon-dev)
padding: 16px
overflow-x: auto
margin: 24px 0
font-size: 13px
line-height: 1.7
```

#### Code Block with Header (coppermoon-dev)

```
┌─────────────────────────────────┐
│  ● ● ●    filename.lua         │  ← bg-zinc-900/50, border-b
├─────────────────────────────────┤
│  code content                   │  ← bg-[#0d1117]
│  ...                            │
└─────────────────────────────────┘
```

#### Syntax Highlighting Palette

| Token      | Color         | Tailwind        |
|------------|---------------|-----------------|
| Keywords   | Purple        | `text-purple-400`|
| Functions  | Amber         | `text-amber-300` |
| Strings    | Green         | `text-emerald-400`|
| Numbers    | Copper        | `text-copper-400`|
| Comments   | Gray          | `text-zinc-500`  |
| Operators  | Gray          | `text-zinc-500`  |
| Variables  | Light         | `text-zinc-200`  |

---

## 9. Layout Patterns

### Navigation

#### Fixed Header (coppermine)

```
position: fixed
width: 100%
height: 64px
background: rgba(0, 0, 0, 0.8)
backdrop-filter: blur(12px)
border-bottom: 1px solid var(--border-primary)
z-index: 100
padding: 0 24px
```

Structure: `[Logo] — [Search] — [Links]`

#### Glass Navbar (coppermoon-dev)

```
position: fixed
z-index: 50
glass effect (bg-white/[0.03] backdrop-blur-xl border-white/[0.06])
responsive: hidden md:flex for desktop links
```

Structure: `[Logo] — [Nav Links] — [CTA Button]`

---

### Sidebar

Documentation sidebar used in `coppermine`:

```
position: fixed
width: 260px
top: 64px (below header)
background: var(--bg-primary)
border-right: 1px solid var(--border-primary)
padding: 24px 0
overflow-y: auto (4px custom scrollbar)
```

#### Sidebar Section

```
Section title: 11px, uppercase, letter-spacing 0.08em, color zinc-500
Links: 14px, color zinc-400, left border 2px transparent
Active: text copper-300, bg copper-glow, left border copper-500
Hover: text zinc-50, bg var(--bg-hover)
```

#### Collapsible Sections

- Chevron rotates 90deg on expand
- Content uses `max-height` + `opacity` transition

---

### Hero Sections

#### Documentation Hero (coppermine)

```
text-align: center
padding: 64px 0
```

- Title: `3.5rem`, bold, gradient text (`white → copper-300 → copper-500`)
- Subtitle: `1.25rem`, `zinc-400`, max-width `600px`
- Actions: centered flex, `gap-12px`

#### Landing Hero (coppermoon-dev)

```
min-height: 90vh
flex items-center justify-center
```

- Background: grid pattern + large copper glow orb (`blur-[128px]`) + floating decorative orbs
- Badge: Pulsing dot + label
- Title: `text-6xl sm:text-7xl md:text-8xl`, shimmer animation
- Subtitle: `text-xl`, `zinc-500`
- CTA: Primary + Secondary buttons
- Code preview: Glass card with glow effect

---

### Content Sections

#### Standard Section

```html
<section class="py-16 lg:py-24 border-t border-zinc-800/50">
  <div class="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8">
    <section-label />
    <h2 />
    <p class="text-zinc-400" />
    <grid / content />
  </div>
</section>
```

#### Feature Grid

```
grid md:grid-cols-2 lg:grid-cols-3 gap-4 lg:gap-6
cards use .reveal-scale for scroll-triggered entrance
```

#### Doc Content Area

```
margin-left: var(--sidebar-width)  /* 260px */
padding: 48px 32px
max-width: 720px
```

---

### Footer

```
margin-top: 64px
padding: 32px 0
border-top: 1px solid var(--border-primary)
text-align: center
font-size: 13px
color: var(--text-tertiary)
```

---

## 10. Responsive Design

### Breakpoints

| Name  | Width    | Tailwind | Usage                         |
|-------|----------|----------|-------------------------------|
| SM    | `640px`  | `sm:`    | Stack → row, padding increase |
| MD    | `768px`  | `md:`    | Tablet layouts, nav visible   |
| LG    | `1024px` | `lg:`    | Desktop layouts, sidebar      |
| XL    | `1280px` | `xl:`    | Large desktop                 |
| 2XL   | `1440px` | `2xl:`   | Extra-wide (TOC panel visible)|

### Responsive Patterns

| Pattern              | Mobile         | Desktop           |
|----------------------|----------------|-------------------|
| Navigation           | Hamburger menu | Horizontal links  |
| Sidebar              | Hidden / toggle| Fixed 260px       |
| Grid                 | 1 col          | 2–3 cols          |
| Hero title           | `text-4xl`     | `text-7xl–8xl`    |
| Section padding      | `py-16`        | `py-24–32`        |
| Horizontal padding   | `px-4`         | `px-6–8`          |
| TOC panel            | Hidden         | Fixed right (1440px+) |
| Card padding         | `p-4–6`       | `p-6–10`          |

---

## 11. Glass Morphism & Effects

### Glass Surface

```css
.glass {
  background: rgba(255, 255, 255, 0.03);  /* white/[0.03] */
  backdrop-filter: blur(24px);             /* backdrop-blur-xl */
  border: 1px solid rgba(255, 255, 255, 0.06);
}

.glass:hover {
  background: rgba(255, 255, 255, 0.06);
  border-color: rgba(255, 255, 255, 0.1);
}
```

### Copper Glow

```css
.glow-copper {
  box-shadow: 0 0 60px -12px rgba(201, 124, 60, 0.25);
}
.glow-copper-sm {
  box-shadow: 0 0 30px -8px rgba(201, 124, 60, 0.2);
}
```

### Grid Background

```css
.grid-bg {
  background-image:
    linear-gradient(rgba(255,255,255,0.02) 1px, transparent 1px),
    linear-gradient(90deg, rgba(255,255,255,0.02) 1px, transparent 1px);
  background-size: 60px 60px;
}
```

### Gradient Overlays

Used to fade decorative elements into the page background:

```css
background: linear-gradient(to bottom, transparent, #000000);
```

### Selection Color

```css
::selection {
  background: rgba(201, 124, 60, 0.3);
  color: copper-200;
}
```

---

## 12. Icons & Assets

### Icon System

- **SVG inline**: Heroicons-style, 24x24 viewBox, `stroke-width="2"` or `fill="currentColor"`
- **Emoji**: Used for ecosystem branding (e.g. tools, features)

### Icon Sizes

| Size   | Dimensions | Tailwind     | Usage             |
|--------|------------|--------------|-------------------|
| XS     | 14x14      | `w-3.5 h-3.5`| Inline indicators|
| SM     | 16x16      | `w-4 h-4`   | Buttons, badges   |
| MD     | 20x20      | `w-5 h-5`   | Navigation        |
| LG     | 24x24      | `w-6 h-6`   | Feature cards     |

### Logo

- Format: PNG + SVG favicon
- Hover: Copper glow effect
- Display height: ~32px in navbar

---

## 13. Syntax Highlighting

Provided by the `highlight` package (highlight.js). Integrated via `highlight_head()` and `highlight_init()`.

### Supported Theme

`github-dark` — aligned with the dark theme.

### Integration

```html
{! highlight_head() !}      <!-- In <head> -->
{! highlight_init() !}      <!-- Before </body> -->
```

---

## 14. Theme Presets

Defined in `packages/tailwind/init.lua`. All presets share the copper palette and component system but differ in accent emphasis.

| Preset         | Inspiration   | Primary Accent | Background |
|----------------|---------------|----------------|------------|
| **coppermoon** | Brand default | Copper `#c97c3c`| `#000000` |
| **vercel**     | Vercel.com    | Minimal white  | `#000000`  |
| **nothing**    | Nothing Phone | Red accent     | `#000000`  |

### Light Theme (copper-blog)

```
background: zinc-50
text: zinc-900
cards: white with shadow
borders: zinc-200
accent: copper-500 / copper-600
```

---

## 15. Implementation Reference

### File Map

| Resource                          | Path                                              |
|-----------------------------------|----------------------------------------------------|
| Shared Tailwind package           | `packages/tailwind/init.lua`                       |
| Shared component definitions      | `packages/tailwind/lib/components.lua`             |
| Coppermine main CSS               | `apps/coppermine/public/css/main.css`              |
| Coppermine base layout            | `apps/coppermine/views/layouts/base.vein`          |
| Coppermine sidebar                | `apps/coppermine/views/partials/sidebar.vein`      |
| Coppermine home page              | `apps/coppermine/views/pages/home.vein`            |
| Coppermine doc page               | `apps/coppermine/views/pages/doc.vein`             |
| Coppermoon-dev base layout        | `apps/coppermoon-dev/views/layouts/base.vein`      |
| Coppermoon-dev home page          | `apps/coppermoon-dev/views/pages/home.vein`        |
| Coppermoon-dev changelog          | `apps/coppermoon-dev/views/pages/changelog.vein`   |
| Copper-blog base layout           | `apps/copper-blog/views/layouts/base.vein`         |
| Highlight package                 | `packages/highlight/`                              |
| Vein template engine              | `packages/vein/`                                   |

### Tailwind Integration

The ecosystem uses a **Lua-based Tailwind wrapper** (`packages/tailwind`) that:

1. Injects Tailwind CDN in development mode
2. Defines custom copper palette, fonts, and component classes
3. Exposes utility functions: `tw.classes()`, `tw.cn()`, `tw.cx()`, `tw.clsx()`
4. Supports preset themes via `tw.preset(name)`
5. Integrates with Vein templates via `{! __tailwind_head !}`

### Component Class Helpers

```lua
tw.classes("btn", "btn_primary")       -- Merge component classes
tw.cn("p-4", condition and "bg-red")   -- Conditional class names
```

### Preset Component Classes

Available via the Tailwind package for consistent usage across apps:

```
btn, btn_primary, btn_secondary, btn_ghost
card, card_title, card_description
input
container, section
heading_1, heading_2, heading_3
prose
badge
alert
```

---

## Quick Reference — Design Tokens

```
Brand:           #c97c3c
Background:      #000000
Surface:         #0a0a0a
Border:          #222222
Text:            #fafafa
Text muted:      #a1a1a1
Font sans:       Geist / Inter
Font mono:       Geist Mono / JetBrains Mono
Base size:       15px
Line height:     1.6
Radius default:  8px
Transition:      200ms ease
Header:          64px
Sidebar:         260px
Content max:     720px
```
