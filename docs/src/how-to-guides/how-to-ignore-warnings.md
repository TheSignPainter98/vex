# How to ignore warnings

1. Check the warning message for the ID of the vex which caused it. In the following example it is `redundant-if`.
    ```
    warning[redundant-if]: if condition is always true
    --> src/my-file.rs:1:1
    |
    1 |         if (true) {
    |           ---------
    |
    ```
2. Put your cursor at the start of the token or block of code which caused it.
3. Type out the following ignore marker:
    - To ignore a specific warning: `// vex:ignore <vex-name-here>`
    - To ignore all warnings: `// vex:ignore *`
