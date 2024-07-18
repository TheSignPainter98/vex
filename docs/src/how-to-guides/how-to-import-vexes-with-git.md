# How to import vexes with git

1. Open the root of your project in the terminal
2. Type the following commands, hitting enter after each:
    ```
    git submodule add <vexes-repo-url> vexes/<vexes-repo-name>
    git add .gitmodules vexes/<vexes-repo-name>
    git commit -m ‘Added vexes’
    ```
