# How to change language

1. Search for the language you want by executing `vex list languages`.
2. Create a section in `vex.toml` called `[<your-language-here>]`.
3. In this section, add the `use-for = []` field.
4. In the square brackets just added, type a [glob][glob] in double-quotes which matches your files.

<!-- TODO(kcza): add glob link here! -->
[glob]: https://?
