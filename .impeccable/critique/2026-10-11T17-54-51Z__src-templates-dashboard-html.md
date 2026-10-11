---
target: dashboard UI
total_score: 24
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:/Users/hugo/Projects/launch2dashboard/src/templates/dashboard.html"
target_fingerprint: "sha256:a5b9f0e375c711f36f0fc3a8693a0b8dd8ea8d2655ac5c4f6716d9b305dd3d8f"
target_path: /Users/hugo/Projects/launch2dashboard/src/templates/dashboard.html
timestamp: 2026-10-11T17-54-51Z
slug: src-templates-dashboard-html
---
Method: dual-agent (A: design review · B: detector). Browser overlay skipped: Chrome extension not connected; review from source.

## Heuristics — 24/40 (Acceptable)
1 Status 3 · 2 Real world 3 · 3 Control 3 · 4 Consistency 2 · 5 Error prevention 3 · 6 Recognition 3 · 7 Flexibility 1 · 8 Minimalism 2 · 9 Error recovery 2 · 10 Help 2

## Specificity
Shell interchangeable with any dark-violet SaaS dashboard; authored details in argument chips, launchd label hint, delete copy. Hides what only this tool could show: plist path, log paths, launchd domain, exit code. "Host: This Mac · localhost" is filler.

## Detector
2 × cramped-padding on <dialog> (#service-dialog, #delete-dialog): false positives (padding lives on children). Minor true positive: #delete-error has no horizontal inset. src/static: clean.

## Priority issues
- [P1] Full list+detail innerHTML rebuild every 3s (app.js:110,129): wipes text selection, breaks screen reader position, focus lost after actions (disabled button) and delete. Fix: diff/patch in place, keep logs owned by updateLogs, aria-disabled. → harden
- [P1] Compact rows truncate names to ~7 chars (icon+badge+2 buttons in ~310px), no title. Fix: drop buttons from compact rows (detail has them), dot instead of badge. → distill/layout
- [P1] Action colours reuse status tints: every running service carries red (Stop), every failed one green (Start). Fix: neutral buttons, coloured icon at most; red reserved for error/Delete. → quieter
- [P2] Action errors in top #page-error cleared by next poll (≤3s, app.js:167). Fix: per-service error slot. → harden
- [P2] Tabs overlap (Overview ⊃ Logs, Config repeats facts). Fix: Overview + Configuration with plist/log paths. → distill

## Personas
Alex: no shortcuts, can't copy logs/paths, only name link selects, truncated names. Sam: focus to body after actions/delete, DOM reset every 3s, skip link targets #main (skips nothing), hung Offline not announced, form errors not tied to fields.

## Minor
Starting has no filter (counts don't sum); two "Live" labels; dead CSS .main-footer; DESIGN.md log height 230px vs 45vh; 100px search ≤600px; perpetual 3s pulse.
