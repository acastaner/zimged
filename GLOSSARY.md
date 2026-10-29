# Glossary

**Image**: the raster file currently open in zimged's single window.

**Annotation**: a shape, text, highlight or obfuscation placed over the Image. It stays a separate, editable object (select, move, resize, restyle, delete) for the rest of the session.
_Avoid_: layer, drawing, markup.

**Flatten**: burning every Annotation into the Image's pixels. Saving and copying to the clipboard always flatten. zimged never stores Annotations in a file of their own.

**Image operation**: an edit that changes the Image's pixels or geometry directly (crop, rotate, flip, resize), as opposed to adding an Annotation.

**Sibling**: another image file in the same directory as the open Image. Moving to the next/previous Sibling is how zimged works as a viewer.
_Avoid_: neighbour, adjacent file.
