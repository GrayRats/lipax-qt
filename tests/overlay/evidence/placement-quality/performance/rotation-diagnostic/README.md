Functional RapidOCR comparison, not a timing benchmark.

The committed Inter Cyrillic fixture was rotated 180° and recognized with the
preserved baseline and release candidate. Both used RapidOCR, the installed
eslav-mobile model and the same explicit synthetic fitting text; no external
translation engine was contacted.

Baseline retained the semantic fragment order: x317 prefix “Привет, как твои
дела”, then x0 suffix “a сегодня? Быстрая ли”. Both fragments share y7, heights
45/46. The candidate's ascending source-coordinate order reverses these fragments
on an upside-down line. Raw source quads therefore cannot establish semantic
reading direction without the crop-orientation decision.

The JSON's individual latency samples are incidental diagnostics and must not be
used as a speed comparison.
