# How to override a file’s language

1. Open a terminal, type and run---
    ```bash
    vex list languages
    ```
2. Find the name of the desired language in the list.
3. Open `vex.toml`.
4. If absent, on a new line at the end of the file, type out a new section `[<language-name>]`.
5. In this section, if absent, type out a new `use-for = []` field.
6. In the square brackets from the previous step, type a [glob][glob] in double-quotes which matches the desired file.

[glob]: ../reference-materials/globs.md
