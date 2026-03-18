---
type: quick
description: Add a built-in shell profile for viewing shell files (*.sh, *rc, justfile) with syntax highlighting
---

<objective>
Add a built-in "shell" profile that color-codes shell script syntax (comments, variables, keywords, strings, shebangs, functions) and auto-selects when viewing shell-related files.

Purpose: Shell files (.sh, .bashrc, .zshrc, justfile) are common developer files that benefit from syntax highlighting when viewed with ctail. Currently they render without any coloring.

Challenge: Files like `.bashrc`, `.zshrc`, and `justfile` don't have standard dot-separated extensions that `Path::extension()` can extract. The auto-selection system needs to also support filename pattern matching in addition to extension matching.

Output: Built-in "shell" profile with filename-aware auto-selection.
</objective>

<context>
@src/config.rs - ProfileConfig struct, builtin_profiles(), find_profile_by_extension(), print_profiles_to()
@src/main.rs - run() function, profile resolution, extension auto-select logic (lines 106-121)
@src/rules.rs - build_rules_with_config
@tests/integration.rs - integration test patterns
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add filename pattern matching to profile auto-selection and create shell profile</name>
  <files>src/config.rs, src/main.rs</files>
  <action>
**In src/config.rs — ProfileConfig struct:**

1. Add a `filenames` field to `ProfileConfig` for filename pattern matching (files without standard extensions):
   ```rust
   /// Filename patterns that auto-select this profile (e.g., ["*rc", "justfile"]).
   /// Supports glob-like suffix matching: "*rc" matches ".bashrc", ".zshrc", etc.
   #[serde(default)]
   pub filenames: Vec<String>,
   ```

**In src/config.rs — builtin_profiles():**

2. Add all existing built-in profiles' `filenames` field as `vec![]` (empty, since they only use extensions).

3. Add a new "shell" profile entry in `builtin_profiles()`:
   - `extensions`: `["sh", "bash", "zsh", "fish", "ksh", "csh"]`
   - `filenames`: `["justfile", "Justfile", "Makefile", "makefile", "GNUmakefile", "*rc"]`
     Note: The `*rc` pattern will match any filename ending in "rc" (like .bashrc, .zshrc, .vimrc).
   - Rules (shell syntax highlighting):
     - `^#!.*` : `dim+cyan` : `line` (shebang line)
     - `^#.*` : `dim` : `line` (comments)
     - `\$\{[^}]+\}` : `cyan` : `match` (variable expansion ${VAR})
     - `\$[A-Za-z_][A-Za-z0-9_]*` : `cyan` : `match` (variable $VAR)
     - `\b(if|then|else|elif|fi|for|do|done|while|until|case|esac|in|function|return|exit|local|export|source|eval)\b` : `magenta` : `match` (keywords)
     - `"[^"]*"` : `green` : `match` (double-quoted strings)
     - `'[^']*'` : `green` : `match` (single-quoted strings)
     - `\||\|\||&&|>>|<<|[<>]` : `yellow` : `match` (pipes and redirections)
     - `^\s*[a-zA-Z_][a-zA-Z0-9_-]*\s*\(\)` : `bold+blue` : `match` (function definitions)
     - `^\s*[a-zA-Z_][a-zA-Z0-9_-]*:` : `bold+yellow` : `match` (make/just targets)
   - `lines`: `Some("+1".to_string())` (show full file, same as markdown/yaml/toml)
   - `trigger`, `before`, `after`: empty/None

**In src/config.rs — find_profile_by_extension():**

4. Rename to `find_profile_by_file` (or add a new companion function `find_profile_by_filename`) that checks BOTH:
   - Extension matching (existing behavior)
   - Filename matching against the `filenames` field: for each filename pattern in the profile, check if the file's name matches. For patterns starting with `*`, do suffix matching (e.g., `*rc` matches a filename ending with "rc"). For exact patterns, do exact match on the filename (e.g., "justfile" matches "justfile").

   The cleanest approach: add a new function alongside `find_profile_by_extension`:
   ```rust
   pub fn find_profile_by_filename(
       filename: &str,
       merged_profiles: &HashMap<String, ProfileConfig>,
   ) -> Option<String> {
       for (name, profile) in merged_profiles {
           for pattern in &profile.filenames {
               if pattern.starts_with('*') {
                   // Suffix match: *rc matches .bashrc, .zshrc, etc.
                   let suffix = &pattern[1..];
                   if filename.ends_with(suffix) {
                       return Some(name.clone());
                   }
               } else if filename == pattern {
                   // Exact match: justfile, Makefile
                   return Some(name.clone());
               }
           }
       }
       None
   }
   ```

**In src/config.rs — print_profile_entry():**

5. Update `print_profile_entry` to also show filename patterns if non-empty, on the auto: line alongside extensions:
   Format: `auto: .sh, .bash, .zsh, .fish, .ksh, .csh, *rc, justfile, Justfile, Makefile`
   (extensions get the dot prefix, filenames are shown as-is)

**In src/main.rs — profile auto-selection (lines 106-121):**

6. Update the auto-select chain to also try filename matching when extension matching fails:
   ```rust
   } else if let Some(ref file_path) = cli.file {
       let path = std::path::Path::new(file_path);
       // Try extension first, then filename patterns
       path.extension()
           .and_then(|ext| ext.to_str())
           .and_then(|ext| config::find_profile_by_extension(ext, &merged_profiles))
           .or_else(|| {
               path.file_name()
                   .and_then(|name| name.to_str())
                   .and_then(|name| config::find_profile_by_filename(name, &merged_profiles))
           })
   }
   ```
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo build 2>&1</automated>
  </verify>
  <done>Built-in shell profile exists in builtin_profiles(). Filename-based auto-selection works for files without extensions. `ctail script.sh`, `ctail .bashrc`, and `ctail justfile` all auto-select the shell profile.</done>
</task>

<task type="auto">
  <name>Task 2: Add tests for shell profile and filename-based auto-selection</name>
  <files>src/config.rs, tests/integration.rs</files>
  <action>
**In src/config.rs unit tests:**

1. Test `builtin_profiles()` returns a "shell" profile with expected extensions and filenames:
   ```rust
   #[test]
   fn builtin_profiles_contains_shell() {
       let profiles = builtin_profiles();
       assert!(profiles.contains_key("shell"));
       let shell = &profiles["shell"];
       assert!(!shell.rules.is_empty());
       assert!(shell.extensions.contains(&"sh".to_string()));
       assert!(shell.extensions.contains(&"bash".to_string()));
       assert!(shell.filenames.contains(&"*rc".to_string()));
       assert!(shell.filenames.contains(&"justfile".to_string()));
       assert_eq!(shell.lines, Some("+1".to_string()));
   }
   ```

2. Test `find_profile_by_filename` with suffix patterns:
   - `.bashrc` matches `*rc` -> shell
   - `.zshrc` matches `*rc` -> shell
   - `justfile` exact match -> shell
   - `Makefile` exact match -> shell
   - `random.txt` matches nothing -> None

3. Test that extension matching still works for .sh files:
   - `find_profile_by_extension("sh", ...)` -> Some("shell")

**In tests/integration.rs:**

4. Test: `ctail` with a `.sh` file auto-selects shell profile and applies ANSI color codes:
   Create a temp `test.sh` file with `#!/bin/bash\n# comment\necho $HOME\n`, run `ctail --color always test.sh` via StdCommand with Stdio::null stdin, verify output contains ANSI codes.

5. Test: `ctail` with a file named `.bashrc` auto-selects shell profile:
   Create a temp file named `.bashrc` with `export PATH=$HOME/bin:$PATH\n# a comment\n`, run `ctail --color always .bashrc` via StdCommand, verify output contains ANSI codes (proving auto-selection worked via filename pattern).

6. Test: `ctail` with a `justfile` auto-selects shell profile:
   Create a temp file named `justfile` with `build:\n  echo "building"\n`, run `ctail --color always justfile` via StdCommand, verify output contains ANSI codes.

7. Test: shell auto-select shows full file (same as markdown's +1 behavior):
   Create a temp `.sh` file with 25+ lines, run `ctail --color never file.sh`, verify all lines appear.
  </action>
  <verify>
    <automated>cd /Users/gimmi/Documents/Sources/rust/ctail && cargo test 2>&1</automated>
  </verify>
  <done>All tests pass. Shell profile auto-selects for .sh, .bashrc, .zshrc, justfile, Makefile. Filename pattern matching (*rc suffix, exact matches) works correctly. Full file is shown by default.</done>
</task>

</tasks>

<verification>
1. `cargo build` succeeds
2. `cargo test` passes all existing + new tests
3. Manual: `ctail some_script.sh` shows full file with colored comments/variables/keywords
4. Manual: create a `.bashrc` temp file and run `ctail .bashrc` -- should auto-select shell profile
5. Manual: `ctail --list-profiles` shows "shell (built-in)" with extensions and filename patterns
</verification>

<success_criteria>
- `ctail script.sh` auto-selects shell profile, shows full file with syntax-highlighted comments/variables/keywords
- `ctail .bashrc` and `ctail .zshrc` auto-select shell via `*rc` filename pattern matching
- `ctail justfile` and `ctail Makefile` auto-select shell via exact filename matching
- `ctail --profile shell` works explicitly for piped input
- `ctail --list-profiles` shows shell as built-in with both extensions and filename patterns on the auto: line
- All existing tests continue to pass
- Filename pattern matching does not interfere with extension-based matching (extension checked first)
</success_criteria>
