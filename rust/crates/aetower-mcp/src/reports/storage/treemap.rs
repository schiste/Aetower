use super::*;

#[derive(Clone, Debug, Default)]
struct StorageTreemapAccumulator {
    path: String,
    label: String,
    depth: usize,
    size_bytes: u64,
    item_count: usize,
    kind_bytes: BTreeMap<String, u64>,
    color_bytes: BTreeMap<String, u64>,
    children: BTreeMap<String, StorageTreemapAccumulator>,
}

impl StorageTreemapAccumulator {
    fn new(path: String, label: String, depth: usize) -> Self {
        Self {
            path,
            label,
            depth,
            ..Self::default()
        }
    }

    fn add_item(&mut self, item: &StorageHygieneItem, components: &[String]) {
        self.size_bytes = self.size_bytes.saturating_add(item.size_bytes);
        self.item_count = self.item_count.saturating_add(1);
        add_bytes(&mut self.kind_bytes, &item.kind, item.size_bytes);
        add_bytes(
            &mut self.color_bytes,
            &storage_treemap_color_key(item),
            item.size_bytes,
        );

        if components.is_empty() || self.depth >= STORAGE_TREEMAP_MAX_DEPTH {
            return;
        }

        let segment = components[0].clone();
        let child_path = Path::new(&self.path).join(&segment).display().to_string();
        let child = self.children.entry(segment.clone()).or_insert_with(|| {
            StorageTreemapAccumulator::new(child_path, segment, self.depth.saturating_add(1))
        });
        child.add_item(item, &components[1..]);
    }
}

pub(super) fn build_storage_treemap_roots(
    items: &[StorageHygieneItem],
    roots: &[String],
) -> Vec<StorageTreemapNode> {
    let normalized_roots = roots
        .iter()
        .map(|root| PathBuf::from(root).display().to_string())
        .collect::<Vec<_>>();
    let mut root_nodes = BTreeMap::<String, StorageTreemapAccumulator>::new();

    for item in items.iter().take(STORAGE_TREEMAP_MAX_ITEMS) {
        let root_path = storage_treemap_root_for_item(item, &normalized_roots);
        let root_label = storage_treemap_label_for_path(&root_path);
        let components = storage_treemap_components(&item.path, &root_path);
        let root = root_nodes
            .entry(root_path.clone())
            .or_insert_with(|| StorageTreemapAccumulator::new(root_path, root_label, 0));
        root.add_item(item, &components);
    }

    let mut roots = root_nodes
        .into_values()
        .map(storage_treemap_node_from_accumulator)
        .collect::<Vec<_>>();
    roots.sort_by(storage_treemap_node_sort);
    roots
}

fn storage_treemap_node_from_accumulator(
    accumulator: StorageTreemapAccumulator,
) -> StorageTreemapNode {
    let mut children = accumulator
        .children
        .into_values()
        .collect::<Vec<StorageTreemapAccumulator>>();
    children.sort_by(|left, right| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.label.cmp(&right.label))
    });
    let has_more = children.len() > STORAGE_TREEMAP_MAX_CHILDREN;
    let mut visible_children = children
        .iter()
        .take(STORAGE_TREEMAP_MAX_CHILDREN)
        .cloned()
        .map(storage_treemap_node_from_accumulator)
        .collect::<Vec<_>>();
    if has_more {
        let overflow = children.iter().skip(STORAGE_TREEMAP_MAX_CHILDREN).fold(
            StorageTreemapAccumulator::new(
                format!("{}/__other", accumulator.path),
                "Other".to_owned(),
                accumulator.depth.saturating_add(1),
            ),
            |mut overflow, child| {
                overflow.size_bytes = overflow.size_bytes.saturating_add(child.size_bytes);
                overflow.item_count = overflow.item_count.saturating_add(child.item_count);
                merge_bytes(&mut overflow.kind_bytes, &child.kind_bytes);
                merge_bytes(&mut overflow.color_bytes, &child.color_bytes);
                overflow
            },
        );
        visible_children.push(storage_treemap_node_from_accumulator(overflow));
    }

    let file_type = dominant_key(&accumulator.kind_bytes).unwrap_or_else(|| "unknown".to_owned());
    let color_key = dominant_key(&accumulator.color_bytes).unwrap_or_else(|| "other".to_owned());
    let node_type = if accumulator.depth == 0 {
        "root"
    } else if visible_children.is_empty() {
        "item"
    } else {
        "folder"
    }
    .to_owned();

    StorageTreemapNode {
        id: accumulator.path.clone(),
        path: accumulator.path,
        label: accumulator.label,
        depth: accumulator.depth,
        node_type,
        file_type,
        color_key,
        size_bytes: accumulator.size_bytes,
        item_count: accumulator.item_count,
        children: visible_children,
        has_more,
    }
}

fn storage_treemap_node_sort(left: &StorageTreemapNode, right: &StorageTreemapNode) -> Ordering {
    right
        .size_bytes
        .cmp(&left.size_bytes)
        .then_with(|| left.path.cmp(&right.path))
}

fn storage_treemap_root_for_item(item: &StorageHygieneItem, roots: &[String]) -> String {
    roots
        .iter()
        .filter(|root| item.path == **root || item.path.starts_with(&format!("{root}/")))
        .max_by_key(|root| root.len())
        .cloned()
        .or_else(|| item.attribution.repo_root.clone())
        .unwrap_or_else(|| {
            Path::new(&item.path)
                .parent()
                .map(|parent| parent.display().to_string())
                .unwrap_or_else(|| item.path.clone())
        })
}

fn storage_treemap_components(path: &str, root: &str) -> Vec<String> {
    let path = Path::new(path);
    let root = Path::new(root);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .filter(|component| !component.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if components.is_empty()
        && let Some(name) = path.file_name().and_then(|name| name.to_str())
    {
        components.push(name.to_owned());
    }
    components
}

fn storage_treemap_label_for_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_owned())
}

fn storage_treemap_color_key(item: &StorageHygieneItem) -> String {
    let kind = item.kind.as_str();
    if item.cleanup_tier == "risky" || item.safety == "risky" {
        "risky"
    } else if matches!(item.cleanup_tier.as_str(), "safe" | "rebuildable") {
        match kind {
            kind if kind.contains("xcode") || kind.contains("swift") => "xcode",
            kind if kind.contains("rust") || kind.contains("cargo") => "rust",
            kind if kind.contains("node") || kind.contains("npm") || kind.contains("pnpm") => {
                "node"
            }
            kind if kind.contains("docker") => "docker",
            kind if kind.contains("log") => "log",
            kind if kind.contains("app") => "app",
            kind if kind.contains("system") || kind.contains("snapshot") => "system",
            kind if kind.contains("large") || kind.contains("cold") => "file",
            _ => "cache",
        }
    } else if item.cleanup_tier == "expensive" {
        "expensive"
    } else {
        "other"
    }
    .to_owned()
}
