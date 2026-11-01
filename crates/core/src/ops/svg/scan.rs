//! What an SVG file is made of: which elements draw, what a `use` points at, and how many shapes a file would draw,
//! counted before anything is expanded.

use std::collections::{HashMap, HashSet};

use roxmltree::{Document, Node, NodeId};

/// The SVG namespace, the only one whose elements draw.
const SVG: &str = "http://www.w3.org/2000/svg";
/// The namespace of `xlink:href`.
const XLINK: &str = "http://www.w3.org/1999/xlink";

/// The local name of `node` if it is an element in the SVG namespace, else "".
pub(super) fn tag<'a>(node: Node<'a, '_>) -> &'a str {
    if node.is_element() && node.tag_name().namespace() == Some(SVG) {
        node.tag_name().name()
    } else {
        ""
    }
}

/// Whether an element called `tag` is a shape.
pub(super) fn is_shape(tag: &str) -> bool {
    matches!(tag, "rect" | "circle" | "ellipse" | "polygon" | "polyline" | "path")
}

/// The elements inside `node` that draw where they stand: shapes, groups and `use`. `defs`, `symbol` and the like draw
/// only through a `use`, and an element that is not a container has no children that draw.
pub(super) fn drawn_children<'a, 'i>(node: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    if !matches!(tag(node), "svg" | "g" | "a" | "switch" | "symbol") {
        return Vec::new();
    }
    node.children()
        .filter(|child| {
            let name = tag(*child);
            is_shape(name) || matches!(name, "svg" | "g" | "a" | "switch" | "use")
        })
        .collect()
}

/// The elements of a file by `id`, for a `use` to find what it points at.
pub(super) struct Index<'a, 'i> {
    by_id: HashMap<&'a str, Node<'a, 'i>>,
}

impl<'a, 'i> Index<'a, 'i> {
    /// The index of `document`; of elements sharing an id, the first.
    pub(super) fn new(document: &'a Document<'i>) -> Self {
        let mut by_id = HashMap::new();
        for node in document.descendants().filter(Node::is_element) {
            if let Some(id) = node.attribute("id") {
                by_id.entry(id).or_insert(node);
            }
        }
        Self { by_id }
    }

    /// The element the `use` element `node` points at, if its `href` names an element of the file.
    pub(super) fn target(&self, node: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
        let link = node.attribute("href").or_else(|| node.attribute((XLINK, "href")))?;
        self.by_id.get(link.strip_prefix('#')?).copied()
    }
}

/// How many shapes drawing `root` makes, each `use` counting as the shapes of what it uses; nothing is expanded, and
/// what a `use` repeats is worked out once.
struct Counter<'a, 'i> {
    index: &'a Index<'a, 'i>,
    depth: usize,
    /// The shapes a `use` of an element draws, for the elements used so far.
    sizes: HashMap<NodeId, u64>,
    /// The elements being counted through a `use`: one met again contains itself.
    working: HashSet<NodeId>,
}

impl<'a, 'i> Counter<'a, 'i> {
    /// The shapes in `node` and below, at `depth` levels of nesting (a `use` is the parent of what it uses).
    fn size(&mut self, node: Node<'a, 'i>, depth: usize) -> Result<u64, String> {
        if depth > self.depth {
            return Err(format!(
                "is nested deeper than {} levels (deep nesting is refused)",
                self.depth
            ));
        }
        let mut total = u64::from(is_shape(tag(node)));
        for child in drawn_children(node) {
            total = total.saturating_add(self.size(child, depth + 1)?);
        }
        if let (true, Some(target)) = (tag(node) == "use", self.index.target(node)) {
            total = total.saturating_add(self.used(target, depth + 1)?);
        }
        Ok(total)
    }

    /// The shapes a `use` of `target` draws.
    fn used(&mut self, target: Node<'a, 'i>, depth: usize) -> Result<u64, String> {
        if let Some(&known) = self.sizes.get(&target.id()) {
            return Ok(known);
        }
        if !self.working.insert(target.id()) {
            return Err("has a use that contains itself".to_owned());
        }
        let size = self.size(target, depth)?;
        self.working.remove(&target.id());
        self.sizes.insert(target.id(), size);
        Ok(size)
    }
}

/// How many shapes drawing `root` makes, counting what each `use` repeats before anything is expanded. Nesting is followed
/// at most `depth` levels down, so the recursion is as shallow.
///
/// # Errors
/// A message if elements nest deeper than `depth` (a `use` is the parent of what it uses) or a `use` contains itself.
pub(super) fn shapes(root: Node<'_, '_>, index: &Index<'_, '_>, depth: usize) -> Result<u64, String> {
    Counter {
        index,
        depth,
        sizes: HashMap::new(),
        working: HashSet::new(),
    }
    .size(root, 1) // constant: the root element is the first level
}
