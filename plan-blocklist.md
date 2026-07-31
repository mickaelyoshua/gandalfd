# Plan: Refactor Blocklist to Reverse Label Trie

Context — Current `AHashSet` checks exact matches. DNS ad blockers require subdomain matching (e.g., `ads.com` blocks `sub.ads.com`). Iterating labels with `AHashSet` is slow.
Objective — Implement a reverse label Trie for fast `O(L)` subdomain resolution.
Approach — Replace `AHashSet` with a custom `Node` struct containing `ahash::HashMap`. Labels are inserted and searched in reverse order.

## Steps

1. Add Imports
- File: `src/blocklist.rs`
- Changes: Import `HashMap` and `Borrow`.
```rust
use std::collections::HashMap;
use std::borrow::Borrow;
use ahash::RandomState;
```

2. Create Node Struct
- File: `src/blocklist.rs`
- Changes: Add `Node` to represent Trie nodes.
```rust
#[derive(Default)]
struct Node {
    terminal: bool,
    children: HashMap<String, Node, RandomState>,
}
```

3. Update Blocklist Struct
- File: `src/blocklist.rs`
- Changes: Replace `domains` with `root`.
```rust
pub struct Blocklist {
    root: Node,
}
```

4. Update `parse_list`
- File: `src/blocklist.rs`
- Changes: Split domains by `.` and reverse. Insert into Trie.
```rust
impl Blocklist {
    pub fn parse_list(raw_data: &str) -> Self {
        let mut root = Node::default();

        for line in raw_data.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }

            if let Some(domain_str) = line.split_whitespace().last() {
                if domain_str == "localhost" || domain_str == "broadcasthost" { continue; }

                if let Ok(d) = DomainName::try_from(domain_str) {
                    let mut current = &mut root;
                    let s: &str = d.borrow();
                    for label in s.split('.').rev() {
                        current = current.children.entry(label.to_string()).or_default();
                    }
                    current.terminal = true;
                }
            }
        }
        Self { root }
    }
}
```

5. Update `is_blocked`
- File: `src/blocklist.rs`
- Changes: Traverse Trie in reverse label order. Return true if `terminal` is reached.
```rust
impl Blocklist {
    pub fn is_blocked(&self, domain: DomainRef<'_>) -> bool {
        let mut current = &self.root;
        for label in domain.as_str().split('.').rev() {
            if let Some(next) = current.children.get(label) {
                if next.terminal { return true; }
                current = next;
            } else {
                return false;
            }
        }
        false
    }
}
```

6. Run Tests
- Command: `cargo test`
- Validation: Ensure all existing hit/miss and ignore_localhost tests pass.
