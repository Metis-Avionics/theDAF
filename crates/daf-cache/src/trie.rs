use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrieNode {
    pub children: HashMap<char, TrieNode>,
    pub key: Option<String>,
}

pub fn trie_insert(root: &mut TrieNode, key: &str) {
    let mut node = root;
    for ch in key.chars() {
        node.children.entry(ch).or_default();
        node = node.children.get_mut(&ch).unwrap();
    }
    node.key = Some(key.to_string());
}

pub fn trie_delete(root: &mut TrieNode, key: &str) {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return;
    }
    let mut path: Vec<*mut TrieNode> = Vec::new();
    let mut current: *mut TrieNode = root;

    for &ch in &chars {
        unsafe {
            let node = &mut *current;
            if !node.children.contains_key(&ch) {
                return;
            }
            path.push(current);
            let child = node.children.get_mut(&ch).unwrap();
            current = child as *mut TrieNode;
        }
    }

    unsafe {
        let node = &mut *current;
        if node.key.as_deref() != Some(key) {
            return;
        }
        node.key = None;

        for i in (0..path.len()).rev() {
            let parent = &mut *path[i];
            let ch = chars[i];
            let should_remove = {
                let child = parent.children.get(&ch).unwrap();
                child.key.is_none() && child.children.is_empty()
            };
            if should_remove {
                parent.children.remove(&ch);
            } else {
                break;
            }
        }
    }
}

pub fn trie_collect(root: &TrieNode, prefix: &str) -> std::collections::HashSet<String> {
    let mut node = Some(root);
    for ch in prefix.chars() {
        node = node.and_then(|n| n.children.get(&ch));
    }
    dfs_collect(node)
}

pub fn trie_delete_prefix(root: &mut TrieNode, prefix: &str) -> std::collections::HashSet<String> {
    if prefix.is_empty() {
        let keys = dfs_collect(Some(root));
        *root = TrieNode::default();
        return keys;
    }

    let chars: Vec<char> = prefix.chars().collect();
    let mut path: Vec<*mut TrieNode> = Vec::new();
    let mut current: *mut TrieNode = root;

    for &ch in &chars {
        unsafe {
            let node = &mut *current;
            if !node.children.contains_key(&ch) {
                return std::collections::HashSet::new();
            }
            path.push(current);
            let child = node.children.get_mut(&ch).unwrap();
            current = child as *mut TrieNode;
        }
    }

    unsafe {
        let node = &mut *current;
        let keys = dfs_collect(Some(node));

        let last_idx = path.len() - 1;
        let parent = &mut *path[last_idx];
        let ch = chars[last_idx];
        parent.children.remove(&ch);

        for i in (0..path.len() - 1).rev() {
            let ancestor = &mut *path[i];
            let ch = chars[i];
            let should_remove = {
                let child = ancestor.children.get(&ch).unwrap();
                child.key.is_none() && child.children.is_empty()
            };
            if should_remove {
                ancestor.children.remove(&ch);
            } else {
                break;
            }
        }

        keys
    }
}

pub fn dfs_collect(node: Option<&TrieNode>) -> std::collections::HashSet<String> {
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

impl PartialOrd for AStarEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn astar_collect(root: &TrieNode, target: &str) -> std::collections::HashSet<String> {
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
        if entry.match_len > 0 && entry.node.key.is_some() {
            if entry.match_len > best_match_len {
                best_match_len = entry.match_len;
                best_keys = std::collections::HashSet::from([entry.node.key.clone().unwrap()]);
            } else if entry.match_len == best_match_len {
                best_keys.insert(entry.node.key.clone().unwrap());
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
