# How to change language

1. Search for the language you want by executing `vex list languages`.
2. Open `vex.toml`.
3. If absent, at the end of the file, type out a new section `[<your-language-here>]`.
4. In this section, if absent, type out a new `use-for = []` field.
5. In the square brackets from the previous instruction, type a [glob][glob] in double-quotes which matches the file whose language association you wish to change.

[glob]: ../reference-materials/globs.md
