use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub key: Option<String>,
}

pub fn trie_insert(root: &mut TrieNode, key: &str) {
    debug_assert!(!key.is_empty(), "trie key must not be empty");
    let mut node = root;
    for ch in key.chars() {
        node = node.children.entry(ch).or_default();
    }
    node.key = Some(key.to_string());
}

pub fn trie_delete(root: &mut TrieNode, key: &str) {
    debug_assert!(!key.is_empty(), "trie key must not be empty");
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return;
    }
    let mut current = root;
    for &ch in &chars {
        if let Some(child) = current.children.get_mut(&ch) {
            current = child;
        } else {
            return;
        }
    }
    if current.key.as_deref() != Some(key) {
        return;
    }
    current.key = None;
}

pub fn trie_collect(root: &TrieNode, prefix: &str) -> std::collections::HashSet<String> {
    debug_assert!(
        !prefix.is_empty() || root.key.is_none(),
        "empty prefix requires root key to be None"
    );
    let mut node = Some(root);
    for ch in prefix.chars() {
        node = node.and_then(|n| n.children.get(&ch));
    }
    dfs_collect(node)
}

pub fn trie_delete_prefix(root: &mut TrieNode, prefix: &str) -> std::collections::HashSet<String> {
    debug_assert!(
        !prefix.is_empty() || root.key.is_none(),
        "empty prefix requires root key to be None"
    );
    if prefix.is_empty() {
        let keys = dfs_collect(Some(root));
        *root = TrieNode::default();
        return keys;
    }

    let chars: Vec<char> = prefix.chars().collect();

    let mut parent = root;
    for ch in &chars[0..chars.len() - 1] {
        if let Some(child) = parent.children.get_mut(ch) {
            parent = child;
        } else {
            return std::collections::HashSet::new();
        }
    }
    let removed = parent.children.remove(&chars[chars.len() - 1]);

    removed
        .as_ref()
        .map_or_else(std::collections::HashSet::new, |node| {
            dfs_collect(Some(node))
        })
}

pub fn dfs_collect(node: Option<&TrieNode>) -> std::collections::HashSet<String> {
    debug_assert!(node.is_some() || true, "dfs_collect on None is valid");
    let mut result = std::collections::HashSet::new();
    if let Some(node) = node {
        if let Some(ref key) = node.key {
            result.insert(key.clone());
        }
        for child in node.children.values() {
            result.extend(dfs_collect(Some(child)));
        }
    }
    result
}

pub fn bfs_collect(root: &TrieNode) -> std::collections::HashSet<String> {
    debug_assert!(
        root.key.is_none() || !root.children.is_empty(),
        "bfs root invariant"
    );
    let mut result = std::collections::HashSet::new();
    let mut queue = vec![root];
    while let Some(node) = queue.pop() {
        if let Some(ref key) = node.key {
            result.insert(key.clone());
        }
        for child in node.children.values() {
            queue.push(child);
        }
    }
    result
}

#[derive(Eq, PartialEq)]
pub struct AStarEntry {
    pub match_len: usize,
    pub counter: usize,
    pub node: TrieNode,
    pub depth: usize,
}

impl Ord for AStarEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.match_len
            .cmp(&other.match_len)
            .then(self.counter.cmp(&other.counter))
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for AStarEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        debug_assert!(self.match_len == other.match_len || self.counter != other.counter, "AStarEntry cmp total order");
        Some(self.cmp(other))
    }
}

pub fn astar_collect(root: &TrieNode, target: &str) -> std::collections::HashSet<String> {
    debug_assert!(!target.is_empty(), "astar target must not be empty");
    let target_chars: Vec<char> = target.chars().collect();
    let mut best_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut best_match_len = 0;
    let mut counter = 0;
    let mut heap = std::collections::BinaryHeap::new();

    heap.push(AStarEntry {
        match_len: 0,
        counter: 0,
        node: root.clone(),
        depth: 0,
    });

    while let Some(entry) = heap.pop() {
        if entry.match_len > 0 {
            if let Some(ref key) = entry.node.key {
                if entry.match_len > best_match_len {
                    best_match_len = entry.match_len;
                    best_keys = std::collections::HashSet::from([key.clone()]);
                } else if entry.match_len == best_match_len {
                    best_keys.insert(key.clone());
                }
            }
        }
        for (ch, child) in entry.node.children {
            let child_depth = entry.depth + 1;
            let child_match = if entry.match_len < target_chars.len()
                && entry.match_len == entry.depth
                && ch == target_chars[entry.match_len]
            {
                entry.match_len + 1
            } else {
                entry.match_len
            };
            counter += 1;
            heap.push(AStarEntry {
                match_len: child_match,
                counter,
                node: child,
                depth: child_depth,
            });
        }
    }

    best_keys
}