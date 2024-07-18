# How to ignore warnings

1. Copy the ID of the vex which caused the warning (in the terminal, look for: `warning[vex-id-here]`).
2. Put your cursor at the start of the token or block of code which triggered the warning.
3. Type out a comment with the following text—
    ```
    vex:ignore <vex-id-here>
    ```
