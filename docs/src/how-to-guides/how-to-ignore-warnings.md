# How to ignore warnings

1. In terminal, copy the vex ID to ignore (`warning[<vex-id-here>]`).
2. Open the file containing the code which triggered the warning.
3. Move the cursor to the start of that code.
4. Type out a comment with the following text—
    ```
    vex:ignore <vex-id-here>
    ```
