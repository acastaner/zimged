# OS-native filename ordering

Research for [#6](https://github.com/acastaner/zimged/issues/6), part of the map [#1](https://github.com/acastaner/zimged/issues/1).
Researched 2026-10-29. Source links are pinned to the commit that was read that day.

## Question

The map settles that **Siblings** are "ordered with the OS's native filename comparison where one exists, otherwise natural sort". This note answers:

- What does each platform's file manager actually use to order names?
- On Linux, can zimged detect the desktop and match its file manager, or should it fall back to natural sort?
- Which natural-sort libraries are reputable in Rust, C++ and Go?

## Conclusion

| Platform | File manager | What it uses | Can zimged call the same thing? |
|---|---|---|---|
| Windows | Explorer | `StrCmpLogicalW` (shlwapi), which can be switched off by the `NoStrCmpLogical` policy [W1][W3] | **Yes.** Call `StrCmpLogicalW` directly. |
| macOS | Finder | `-[NSString localizedStandardCompare:]` ("Compares strings as sorted by the Finder") [M1] | **Yes.** Call it directly. |
| Linux, GNOME | Files (Nautilus) | `g_utf8_collate_key_for_filename()` on the display name, compared with `strcmp`. Names starting with `.` or `#` go last [G1][G2] | **Yes**, if GLib is linked. Otherwise port about 150 lines of C. |
| Linux, KDE | Dolphin | Its own compare: `QCollator` (ICU) with numeric mode, case-insensitive, base name before extension, decimal-aware since 26.08 [K1][K2] | **Partly.** `QCollator` is a public API, but Dolphin's wrapper is private code that would need porting (about 200 lines). |
| Other Linux | Thunar, Nemo, COSMIC Files and others | Not researched | Fall back to the GNOME behaviour. |

**Recommendation**

1. Put Sibling ordering behind **one comparator interface** (`compare(a, b)`, plus an optional `sort_key(name)`), with one implementation per platform.
2. **Windows:** call `StrCmpLogicalW`. If the `NoStrCmpLogical` policy is set to 1 (HKCU or HKLM `Software\Microsoft\Windows\CurrentVersion\Policies\Explorer`), use a literal case-insensitive compare instead, as Explorer does [W3].
3. **macOS:** call `localizedStandardCompare:`.
4. **Linux:** read `XDG_CURRENT_DESKTOP` once at startup. If any of its colon-separated entries equals `KDE`, use a port of Dolphin's natural-sort compare. Otherwise use GLib's `g_utf8_collate_key_for_filename` (linked, or ported). That is also the "natural sort" fallback [X1][X2].
5. **Don't** read other apps' configuration (for example Dolphin's `SortingChoice` in `dolphinrc`) in v1. Matching the defaults is enough, and reading it is a cheap later addition.
6. **Performance:** GLib and ICU both produce sort keys. Compute a key once per name, so 10,000 files cost 10,000 key builds plus cheap `memcmp` comparisons, not around 140,000 locale-aware compares. `StrCmpLogicalW` and `localizedStandardCompare:` have no key form, but about 140,000 calls for 10,000 names is still small. Measure it with the cold-start spike. Sort off the UI thread either way, so the first image is never held up (map budget).
7. **Be honest in the spec:** both Microsoft and Apple document that the behaviour "can change from release to release" [W1][M1]. Calling the OS function is the only way to keep matching, which is the argument for native calls over a reimplementation on those two platforms.

## Platform detail

### Windows: Explorer and `StrCmpLogicalW`

- **API.** `int StrCmpLogicalW(PCWSTR psz1, PCWSTR psz2)` in `shlwapi.h` / `Shlwapi.dll`, available since Windows XP. Microsoft describes it this way: "Digits in the strings are considered as numerical content rather than text. This test is not case-sensitive." Its documented example sorts `2string, 3string, 20string, st2ring, st3ring, st20ring, string2, string3, string20` [W1].
- **Stability caveat, quoted:** "Behavior of this function, and therefore the results it returns, can change from release to release. It should not be used for canonical sorting applications." [W1]. That is fine for display ordering. It does mean zimged must not persist anything keyed on the order.
- **Link to Explorer.** Microsoft's own policy reference ties Explorer to this function by name. The `NoStrCmpLogical` ADMX policy ("Turn off numerical sorting in File Explorer") says: "If you enable this policy setting, File Explorer will sort file names by each digit in a file name (for example, 111 < 22 < 3). If you disable or don't configure this policy setting, File Explorer will sort file names by increasing number value (for example, 3 < 22 < 111)." It applies at user and device scope, under registry key `Software\Microsoft\Windows\CurrentVersion\Policies\Explorer`, value `NoStrCmpLogical` [W3].
- **Documented locale-aware alternative.** `CompareStringEx(locale, SORT_DIGITSASNUMBERS | LINGUISTIC_IGNORECASE, …)`. `SORT_DIGITSASNUMBERS` exists since Windows 7 [W2]. It is *not* documented as what Explorer uses, so prefer `StrCmpLogicalW`.
- **Bindings.** Rust: `windows::Win32::UI::Shell::StrCmpLogicalW` in the `windows` crate [W4]. C++/Qt: call it directly. Go: `golang.org/x/sys/windows` does not wrap it; a `syscall.NewLazyDLL("shlwapi.dll")` call is needed (T).
- **Not verified here:** how `StrCmpLogicalW` treats leading zeros, accents and punctuation. The docs don't say, and no Windows host was available for this research. A five-minute test on Windows with the [edge-case list](#edge-cases) is the follow-up.

### macOS: Finder and `localizedStandardCompare:`

- **API.** `- (NSComparisonResult)localizedStandardCompare:(NSString *)string`, in macOS 10.6+. Apple's summary is "Compares strings as sorted by the Finder." The discussion adds: "This method should be used whenever file names or other strings are presented in lists and tables where Finder-like sorting is appropriate. The exact sorting behavior of this method is different under different locales and may be changed in future releases. This method uses the current locale." [M1]
- **What it is made of.** Apple's docs don't list the options. Apple's open-source swift-corelibs-foundation implements it as `compare(_:options: [.caseInsensitive, .numeric, .widthInsensitive, .forcedOrdering], range:, locale: Locale.current)` [M2]. That is a reimplementation for non-Darwin platforms, so treat it as strong evidence, not as a guarantee for Apple's closed Foundation. `.forcedOrdering` means names that differ only by case still get a deterministic order.
- **Bindings.** Rust: `objc2_foundation::NSString::localizedStandardCompare` [M3]. C++/Qt: a small Objective-C++ file. Qt's `QCollator` uses Apple's API on macOS rather than ICU by default [Q1], but it is not documented as Finder-equivalent, so call the NSString method.
- **Not verified here:** Finder's behaviour on real edge cases (no Mac was available).

### Linux, GNOME: Files (Nautilus) and GLib

- **Where.** `nautilus-file.c` computes `display_name_collation_key = g_utf8_collate_key_for_filename (display_name, -1)` whenever the display name is set. `compare_by_display_name()` then puts names whose first byte is `.` or `#` (`SORT_LAST_CHAR1`/`SORT_LAST_CHAR2`) last, and otherwise compares the keys with `strcmp` [G1].
- **The algorithm** (`glib/gunicollate.c`) [G2]. It splits the name into runs. Text runs that contain no `.` or digit go through `g_utf8_collate_key` (NFC-composed, then libc `wcsxfrm`/`strxfrm` in the current locale). Special runs are prefixed with a sentinel `\1\1\1` so that they sort before any libc key:
  - **A dot** becomes `\1`, which is lower than everything else, so `event.c < event.h < eventgenerator.c`.
  - **A digit run** becomes `\2` plus *(digits − 1)* `:` characters plus the digits, so numbers compare by length, then by value. Leading zeros are stripped, and their count is appended as one byte at the **very end** of the key, as a last-resort tie-break.
  - GLib's documentation says the function "depends on the current locale", and the process must have called `setlocale(LC_ALL, "")`. This was confirmed in testing: without it, keys fall back to codepoint order.
- **Stability.** It has been in GLib since 2.8 and the code is long-standing.

### Linux, KDE: Dolphin, KIO and `QCollator`

- **Dolphin's own model, not KIO's proxy.** Dolphin sorts in `KFileItemModel`. Its constructor calls `m_collator.setNumericMode(true)`. `loadSortingSettings()` maps the `SortingChoice` setting, `NaturalSorting` (default, index 0 [K3]), `CaseInsensitiveSorting` and `CaseSensitiveSorting`, to a natural-sort flag plus `Qt::CaseInsensitive` or `Qt::CaseSensitive` [K1].
- **The natural-sort branch** of `KFileItemModel::stringCompare()` [K1] works like this:
  1. Find the extension separator: the **last** `.`, unless it sits between two digits. Compare the **base names** first, and compare extensions only when the base names are equal. This came in with Dolphin 24.12, commit `e4cc6e69` "natural sort: exclude extension when comparing filenames" [K2].
  2. `decimalAwareNaturalCompare()` walks both strings. Text runs go through `QCollator::compare`. ASCII digit runs, including dotted chains such as `1.2.10`, are compared numerically, ignoring leading zeros. A chain of exactly **two** segments (`1.10`) is compared as a **decimal** (`1.10` = 1.1 < 1.5). Three or more segments are compared segment by segment, like versions. This came in with Dolphin **26.08**, commit `0d10eff3` "sort dotted numeric names naturally". It is absent from v26.04.3 and present in v26.08.0 [K2].
  3. When everything compares equal, the full strings are compared with `QString::compare` (case-insensitive, then case-sensitive) to get a deterministic order.
- **Other sort steps** (`lessThan`): folders first (default on), optional "hidden last" (default off), and grouping. Only the name comparison matters for Siblings.
- **KIO (file dialogs).** `KDirSortFilterProxyModel` uses a plain `QCollator` with `setNumericMode(naturalSorting)` and no extension split [K4]. KDE file dialogs therefore order slightly differently from Dolphin. Dolphin is the reference for "the file manager".
- **QCollator.** Numeric mode compares digit runs by value, and works only with the ICU backend. The POSIX fallback doesn't support it and prints a warning [Q1]. ICU's equivalent is `UCOL_NUMERIC_COLLATION` (`kn`), where "sequences of decimal digits (gc=Nd) sort by their numeric value" [I1].

### Desktop detection: `XDG_CURRENT_DESKTOP`

- **Spec.** "If `$XDG_CURRENT_DESKTOP` is set then it contains a colon-separated list of strings", set by the login manager from the session file's `DesktopNames` [X1]. The registered values are **case-sensitive**: `KDE`, `GNOME`, `GNOME-Classic`, `XFCE`, `Cinnamon`, `MATE`, `COSMIC` and others ("`KDE` not `kde`") [X2].
- **Practical shape.** Distributions prepend their own token (Ubuntu sets `ubuntu:GNOME` (T)), so split on `:` and test each entry. Never compare the whole string.
- **Limits.** The variable describes the *session*, not the user's file manager: someone may run Dolphin on GNOME. It is unset under some minimal window managers and inside `sudo` or a terminal launched oddly. Flatpak passes it through (T). The fallback (GLib behaviour) is safe in all of these cases.

## Edge cases

The orderings below were **measured on this Linux host** (GLib 2.88.0, Qt 6.10.2 with ICU, locale `en_US.UTF-8`) with three small programs. The programs used GLib's `g_utf8_collate_key_for_filename`, a bare `QCollator` (numeric, case-insensitive), and **Dolphin master's `stringCompare` natural branch copied verbatim** from `e501e8ca` [K1]. The Windows and macOS columns are from the docs only.

| Case | Nautilus / GLib (measured) | Dolphin ≥ 26.08 (measured) | Bare QCollator numeric (≈ KIO dialog) | Windows `StrCmpLogicalW` | macOS `localizedStandardCompare:` |
|---|---|---|---|---|---|
| **Numbers** `img1, img2, img10` | 1 < 2 < 10 | 1 < 2 < 10 | 1 < 2 < 10 | 1 < 2 < 10 [W1] | 1 < 2 < 10 (`.numeric`) [M2] |
| **Leading zeros** `img2, img02, img002` | `img2 < img02 < img002` (fewer zeros first, final tie-break) | `img002 < img02 < img2` (equal value, then a literal compare where `0` < `2`) | equal, so input order | not documented | not documented |
| **Case** `IMG3, img3, img10` | `img3.png < img3.PNG < img10 < IMG3` (**case inside a text run outranks the following number**, a GLib quirk) | `IMG3 ≈ img3`, both before `img10`. Case only breaks ties | `IMG3 ≈ img3 < img10` | case-insensitive [W1] | case-insensitive [M2] |
| **Accents** `eclair, Éclair, resume, Resume, résumé` | locale-aware: `eclair < Éclair`, `resume < Resume < résumé` | same (ICU) | same | not documented | locale-aware [M1] |
| **Dots and extensions** `a.png, a-b.png, a.b.png, ab.png` | `a.b < a < a-b < ab` (a dot sorts before everything, per segment) | `a < a-b < a.b < ab` (base name first) | `a-b < a.b < a < ab` (punctuation weighed by ICU) | not documented | not documented |
| **Suffix versus copy** `photo.png, photo-1.png` / `file.png, file (1).png` | `photo.png < photo-1.png`, `file.png < file (1).png` | same as GLib | **reversed**: `photo-1 < photo`, `file (1) < file` | not documented | not documented |
| **Same base, different extension** `photo.jpeg, photo.jpg, photo.png` | jpeg < jpg < png | jpeg < jpg < png | jpeg < jpg < png | not documented | not documented |
| **Decimals** `1.5, 1.9, 1.10` | `1.5 < 1.9 < 1.10` (dot as separator) | `1.10 < 1.5 < 1.9` (decimal reading, 26.08+) | `1.5 < 1.9 < 1.10` | not documented | not documented |
| **Leading punctuation** `.hidden, #note, _under` | `.hidden` first by key, but **Nautilus forces `.` and `#` names last** [G1]. `_under` sorts as `under` | `_under < .hidden < #note`, before digits | same as Dolphin | not documented | not documented |

Takeaways:

- **No two implementations agree on leading zeros.** zimged should accept whatever the platform comparator gives. A Linux port should copy the chosen reference exactly, not "improve" on it.
- **The extension rule is the most visible difference** (`photo.png` versus `photo-1.png`). GLib's dot rule and Dolphin's base-name rule agree on common camera and screenshot names. A bare ICU/QCollator compare does not. If Qt is the stack, **don't** use bare `QCollator` and assume it matches Dolphin.
- **The decimal rule (Dolphin 26.08+) is new** and may still change. Pin the port to a Dolphin release and note the commit.

## Natural-sort libraries

These are for the fallback path, or for building the Linux ports. Download and star figures are as of 2026-10-29.

### Rust

| Crate | Version | What it does | Notes |
|---|---|---|---|
| [`icu_collator`](https://crates.io/crates/icu_collator) (ICU4X) | 2.4.0, 12.2 M downloads | Full UCA/CLDR collation. `CollatorPreferences::numeric_ordering` (the `-u-kn` key) turns on numeric digit runs [L1] | Unicode Consortium project. **The right base for a Dolphin port** (QCollator is ICU underneath). Locale data is compiled in, so watch binary size. |
| [`alphanumeric-sort`](https://crates.io/crates/alphanumeric-sort) | 1.5.8, 7.0 M downloads | Digit runs by value, leading zeros as tie-break. `sort_path_slice` for `Path`. "not a locale-aware collation library" [L2] | Maintained (pushed 2026-09). Good simple fallback. No accents or case folding. |
| [`natord`](https://crates.io/crates/natord) | 1.0.9, 11.9 M downloads | Classic `strnatcmp` port, with a case-insensitive variant | Last release 2015. Widely used but unmaintained. |
| [`lexical-sort`](https://crates.io/crates/lexical-sort) | 0.3.1, 5.6 M downloads | Natural compare plus ASCII transliteration of accents | Last release 2020. |
| [`human-sort`](https://crates.io/crates/human-sort) | 0.2.2, 3.7 M downloads | Simple natural compare | Last release 2019. |
| `glib` (gtk-rs) | — | Exposes `g_utf8_collate_key_for_filename` (T) | Only if GTK/GLib is in the stack anyway. |

### C++

| Library | What it does | Notes |
|---|---|---|
| **Qt `QCollator`** with `setNumericMode(true)` | ICU numeric collation, plus case sensitivity and punctuation options [Q1] | Already in the stack if Qt is chosen (#2 ranks Qt first). It is what Dolphin builds on. |
| **ICU** `ucol_setAttribute(coll, UCOL_NUMERIC_COLLATION, UCOL_ON)` | The same thing without Qt [I1] | The reference implementation. `ucol_getSortKey` gives sort keys. |
| **GLib** `g_utf8_collate_key_for_filename` | The exact Nautilus behaviour [G2] | LGPL-2.1+. Linking GLib only for this is heavy without GTK, and porting is easy. |
| **strnatcmp** (Martin Pool, [`sourcefrog/natsort`](https://github.com/sourcefrog/natsort)) | The original C natural compare (zlib licence (T)) | 142 stars, stable, ASCII only. |

### Go

| Package | What it does | Notes |
|---|---|---|
| [`golang.org/x/text/collate`](https://pkg.go.dev/golang.org/x/text/collate) with `collate.Numeric` | Locale-aware CLDR collation with numeric digit runs. The documented example sorts `0 01 2 3 11 23` [L3] | Go-team module. The best base for anything locale-aware. |
| [`github.com/fvbommel/sortorder`](https://github.com/fvbommel/sortorder) | Natural compare (`sortorder.NaturalLess`) | MIT, maintained (2026-10). |
| [`github.com/maruel/natural`](https://github.com/maruel/natural) | Natural compare | Apache-2.0, maintained (2026-10). |
| [`github.com/facette/natsort`](https://github.com/facette/natsort) | Chunked natural sort | BSD-3, last push 2024. |

## Open follow-ups

- Run the edge-case list through `StrCmpLogicalW` on Windows and `localizedStandardCompare:` on macOS, then fill in the "not documented" cells.
- Decide whether the GLib "case inside a run outranks the number" quirk (`IMG3` after `img10`) is worth copying. Copying it gives a faithful match. Not copying it gives the order users would expect.
- Once the stack ADR lands (#2), pick concrete crates or bindings: on Rust, probably `windows`, `objc2-foundation` and `icu_collator`, plus a ported GLib key function.

## Sources

"(T)" marks a claim from prior knowledge that was not re-verified against a primary source in this session.

- **[W1]** Microsoft Learn, *StrCmpLogicalW function (shlwapi.h)*. https://learn.microsoft.com/en-us/windows/win32/api/shlwapi/nf-shlwapi-strcmplogicalw
- **[W2]** Microsoft Learn, *CompareStringEx function*, `SORT_DIGITSASNUMBERS`. https://learn.microsoft.com/en-us/windows/win32/api/stringapiset/nf-stringapiset-comparestringex
- **[W3]** Microsoft Learn, *Policy CSP – ADMX_WindowsExplorer*, `NoStrCmpLogical`. https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-windowsexplorer#nostrcmplogical
- **[W4]** windows-rs docs, `StrCmpLogicalW`. https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Shell/fn.StrCmpLogicalW.html
- **[M1]** Apple Developer, *localizedStandardCompare(_:)*. https://developer.apple.com/documentation/foundation/nsstring/localizedstandardcompare(_:)
- **[M2]** swift-corelibs-foundation, `Sources/Foundation/NSString.swift`, `localizedStandardCompare`. https://github.com/swiftlang/swift-corelibs-foundation/blob/main/Sources/Foundation/NSString.swift
- **[M3]** objc2-foundation docs, `NSString`. https://docs.rs/objc2-foundation/latest/objc2_foundation/struct.NSString.html
- **[G1]** Nautilus `src/nautilus-file.c` at `7f1e3c6f`: the collation key is set where the display name is set, and `compare_by_display_name` puts `.`/`#` names last. https://gitlab.gnome.org/GNOME/nautilus/-/blob/7f1e3c6f608759a749662a9c5bcd8d8a1894d526/src/nautilus-file.c
- **[G2]** GLib `glib/gunicollate.c` at `e09400a9`, `g_utf8_collate_key_for_filename`. https://gitlab.gnome.org/GNOME/glib/-/blob/e09400a9df066f4f0595c02be6ba039252886725/glib/gunicollate.c
- **[K1]** Dolphin `src/kitemviews/kfileitemmodel.cpp` at `e501e8ca` (`decimalAwareNaturalCompare`, `loadSortingSettings`, `stringCompare`). https://invent.kde.org/system/dolphin/-/blob/e501e8caabd35daaaeb176e024f092ab222c1401/src/kitemviews/kfileitemmodel.cpp
- **[K2]** Dolphin commits `e4cc6e694300` (2024-11-19, extension excluded, in v24.12.0) and `0d10eff3726e` (2026-04-02, dotted numerics, in v26.08.0 but not v26.04.3). https://invent.kde.org/system/dolphin/-/commit/e4cc6e694300 · https://invent.kde.org/system/dolphin/-/commit/0d10eff3726e
- **[K3]** Dolphin `src/settings/dolphin_generalsettings.kcfg`, `SortingChoice` (default 0 = `NaturalSorting`). https://invent.kde.org/system/dolphin/-/blob/e501e8caabd35daaaeb176e024f092ab222c1401/src/settings/dolphin_generalsettings.kcfg
- **[K4]** KIO `src/filewidgets/kdirsortfilterproxymodel.cpp` at `d7cf9770`. https://invent.kde.org/frameworks/kio/-/blob/d7cf9770a9190b686c6edb515eed5e84ff3b0f2f/src/filewidgets/kdirsortfilterproxymodel.cpp
- **[Q1]** Qt 6 docs, *QCollator* (numeric mode, POSIX fallback, macOS backend). https://doc.qt.io/qt-6/qcollator.html
- **[I1]** ICU User Guide, *Collation customization*, `numericOrdering` / `UCOL_NUMERIC_COLLATION`. https://unicode-org.github.io/icu/userguide/collation/customization/
- **[X1]** freedesktop.org, *Desktop Entry Specification*, recognised keys (`OnlyShowIn`, `$XDG_CURRENT_DESKTOP`). https://specifications.freedesktop.org/desktop-entry/latest/recognized-keys.html
- **[X2]** freedesktop.org, *Desktop Menu Specification*, Appendix B, registered OnlyShowIn environments. https://specifications.freedesktop.org/menu/latest/onlyshowin-registry.html
- **[L1]** docs.rs, `icu_collator` 2.4.0 `CollatorPreferences` / `CollationNumericOrdering`. https://docs.rs/icu_collator/latest/icu_collator/struct.CollatorPreferences.html
- **[L2]** `alphanumeric-sort` README. https://github.com/magiclen/alphanumeric-sort
- **[L3]** pkg.go.dev, `golang.org/x/text/collate`, `Numeric` example. https://pkg.go.dev/golang.org/x/text/collate
