---
"@parity/truapi": major
"@parity/truapi-host": major
---

A renderer `Text` node can name the four typography styles between `HeadlineLarge` and `TitleMediumRegular`: `HeadlineMedium`, `HeadlineSmall`, `TitleExtraLarge` and `TitleLarge`. They take the next SCALE indexes, so trees that name only the existing styles encode as before.

A host that maps every `TypographyStyle` handles the four new cases. The iOS and Android hosts draw them with the design system tokens of the same names. A host built before this release cannot decode a tree that names one of them. The first such tree ends the render stream it arrived on, and the host draws nothing more from that stream. Other render streams of the same product keep running.
