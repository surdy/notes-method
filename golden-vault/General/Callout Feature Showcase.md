---
title: Callout Feature Showcase
tags:
  - notesmith
  - formatting
  - callouts
aliases:
  - Obsidian Callout Showcase
---

# Callout Feature Showcase

This note demonstrates the Obsidian-style callout behavior that Notesmith supports in Reading View.

## Basic callouts

> [!note]
> A default note callout uses its type identifier as the title when no custom title is provided.

> [!info] Custom title
> Callouts can override the generated title with text after the type identifier.

> [!tip] Title-only callout

## Built-in types

> [!abstract] Abstract
> Summaries and overviews.

> [!todo] Todo
> Work that still needs to happen.

> [!success] Success
> A completed or positive outcome.

> [!question] Question
> Open questions or FAQs.

> [!warning] Warning
> Cautionary context.

> [!failure] Failure
> Failed or missing work.

> [!danger] Danger
> High-risk or urgent information.

> [!bug] Bug
> A defect or known issue.

> [!example] Example
> A concrete example.

> [!quote] Quote
> Quoted or cited context.

## Aliases

These should render with their canonical type styling while preserving the original identifier.

> [!summary] Summary alias
> `summary` renders like `abstract`.

> [!important] Important alias
> `important` renders like `tip`.

> [!done] Done alias
> `done` renders like `success`.

> [!faq] FAQ alias
> `faq` renders like `question`.

> [!attention] Attention alias
> `attention` renders like `warning`.

> [!missing] Missing alias
> `missing` renders like `failure`.

> [!error] Error alias
> `error` renders like `danger`.

> [!cite] Cite alias
> `cite` renders like `quote`.

## Unsupported type fallback

> [!custom-project-status] Unsupported custom type
> Custom callout definitions are not implemented yet, so unsupported types fall back to the `note` style.

## Foldable callouts

> [!faq]- Collapsed by default
> This body should be hidden in Reading View until the callout title is clicked.

> [!info]+ Expanded by default
> This body should be visible initially and collapse when the callout title is clicked.

## Nested callouts

> [!question] Can callouts be nested?
> > [!todo] Yes, they can.
> > Nested callouts render inside the parent callout body.
> > > [!example] Multiple layers
> > > Notesmith converts the innermost callout first, then works outward.
