---
"@parity/truapi": major
"@parity/truapi-host": major
---

A renderer `Text` node can name the four typography styles between `HeadlineLarge` and `TitleMediumRegular`: `HeadlineMedium`, `HeadlineSmall`, `TitleExtraLarge` and `TitleLarge`. They take the next SCALE indexes, so trees that name only the existing styles encode as before.

A host that maps every `TypographyStyle` handles the four new cases. The iOS and Android hosts draw them with the design system tokens of the same names. A host built before this release cannot decode a tree that names one of them. The first such tree fails the render stream it arrived on, and the iOS and Android hosts keep what they last drew for that body, or nothing if they never drew one. They open the stream again, which fails the same way for as long as the product sends that tree, and on iOS a chat or footer card stops after three streams in a row that drew nothing. Other render streams of the same product keep running.
