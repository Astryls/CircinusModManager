//! A document arena for RimWorld's unified Defs XML.
//!
//! RimWorld concatenates every active mod's Defs into one document, runs every patch against
//! it, then resolves inheritance. Both steps mutate the tree, so this is a mutable DOM with the
//! parts of .NET's `XmlDocument` that patches can observe: elements, text nodes, attributes,
//! sibling order. Nodes are small (linked siblings, interned names, one text heap) because a
//! large list turns into millions of them. Every node remembers who created it (`origin`) and,
//! after inheritance, which named node it was copied from (`via`).

use std::cell::RefCell;
use std::collections::HashMap;

pub type NodeId = u32;
/// The document node: parent of the root element, the context node of every patch XPath.
pub const DOCUMENT: NodeId = 0;
pub const NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(pub u32);

#[derive(Debug, Default)]
pub struct Interner {
    map: HashMap<Box<str>, u32>,
    names: Vec<Box<str>>,
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Sym {
        if let Some(&i) = self.map.get(s) {
            return Sym(i);
        }
        let i = self.names.len() as u32;
        self.names.push(s.into());
        self.map.insert(s.into(), i);
        Sym(i)
    }
    pub fn lookup(&self, s: &str) -> Option<Sym> {
        self.map.get(s).map(|&i| Sym(i))
    }
    pub fn get(&self, s: Sym) -> &str {
        &self.names[s.0 as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Document,
    Element,
    Text,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: u32,
    pub first: u32,
    pub last: u32,
    pub prev: u32,
    pub next: u32,
    /// Element: the tag name. Text: start of the text in the heap.
    a: u32,
    /// Element: first attribute (NONE = none). Text: length of the text.
    b: u32,
    /// Index into the caller's origin table: who created this node.
    pub origin: u32,
    /// After inheritance: index into the caller's names table of the named node this was copied
    /// from (0 = the node is the def's own).
    pub via: u32,
    pub kind: Kind,
    /// Detached by a patch. Dead nodes stay in the arena so journal entries can still point at
    /// them, but they are unreachable from the document.
    pub dead: bool,
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: Sym,
    start: u32,
    len: u32,
    next: u32,
}

/// The well-known names patches and inheritance look at.
#[derive(Debug, Clone, Copy)]
pub struct Syms {
    pub def_name: Sym,
    pub li: Sym,
    pub name: Sym,
    pub parent_name: Sym,
    pub abstract_: Sym,
    pub inherit: Sym,
    pub class: Sym,
    pub may_require: Sym,
    pub may_require_any_of: Sym,
    pub defs: Sym,
    pub mod_extensions: Sym,
}

#[derive(Debug, Default)]
struct RootIndex {
    /// (tag, string-value of the defName child) → root children, in document order.
    by_def_name: HashMap<(Sym, Box<str>), Vec<NodeId>>,
    /// (tag, Name attribute) → root children.
    by_name: HashMap<(Sym, Box<str>), Vec<NodeId>>,
}

#[derive(Debug)]
pub struct Doc {
    pub nodes: Vec<Node>,
    attrs: Vec<Attr>,
    text: String,
    pub syms: Interner,
    pub s: Syms,
    root_index: RefCell<Option<RootIndex>>,
}

impl Default for Doc {
    fn default() -> Self {
        Doc::new()
    }
}

impl Doc {
    pub fn new() -> Doc {
        let mut syms = Interner::default();
        let s = Syms {
            def_name: syms.intern("defName"),
            li: syms.intern("li"),
            name: syms.intern("Name"),
            parent_name: syms.intern("ParentName"),
            abstract_: syms.intern("Abstract"),
            inherit: syms.intern("Inherit"),
            class: syms.intern("Class"),
            may_require: syms.intern("MayRequire"),
            may_require_any_of: syms.intern("MayRequireAnyOf"),
            defs: syms.intern("Defs"),
            mod_extensions: syms.intern("modExtensions"),
        };
        let document = Node { parent: NONE, first: NONE, last: NONE, prev: NONE, next: NONE, a: 0, b: NONE, origin: 0, via: 0, kind: Kind::Document, dead: false };
        Doc { nodes: vec![document], attrs: Vec::new(), text: String::new(), syms, s, root_index: RefCell::new(None) }
    }

    /// The root element (`Defs`) once one has been added.
    pub fn root(&self) -> Option<NodeId> {
        self.element_children(DOCUMENT).next()
    }

    // ----- creation -------------------------------------------------------------------------

    fn push(&mut self, n: Node) -> NodeId {
        let id = self.nodes.len() as u32;
        self.nodes.push(n);
        id
    }

    pub fn new_element(&mut self, name: Sym, origin: u32) -> NodeId {
        self.push(Node { parent: NONE, first: NONE, last: NONE, prev: NONE, next: NONE, a: name.0, b: NONE, origin, via: 0, kind: Kind::Element, dead: false })
    }

    pub fn new_element_named(&mut self, name: &str, origin: u32) -> NodeId {
        let s = self.syms.intern(name);
        self.new_element(s, origin)
    }

    fn store_text(&mut self, text: &str) -> (u32, u32) {
        let start = self.text.len() as u32;
        self.text.push_str(text);
        (start, text.len() as u32)
    }

    pub fn new_text(&mut self, text: &str, origin: u32) -> NodeId {
        let (start, len) = self.store_text(text);
        self.push(Node { parent: NONE, first: NONE, last: NONE, prev: NONE, next: NONE, a: start, b: len, origin, via: 0, kind: Kind::Text, dead: false })
    }

    // ----- reading --------------------------------------------------------------------------

    #[inline]
    pub fn kind(&self, n: NodeId) -> Kind {
        self.nodes[n as usize].kind
    }
    #[inline]
    pub fn is_element(&self, n: NodeId) -> bool {
        self.nodes[n as usize].kind == Kind::Element
    }
    #[inline]
    pub fn is_text(&self, n: NodeId) -> bool {
        self.nodes[n as usize].kind == Kind::Text
    }
    /// Tag symbol of an element (meaningless for other kinds).
    #[inline]
    pub fn tag(&self, n: NodeId) -> Sym {
        Sym(self.nodes[n as usize].a)
    }
    pub fn name(&self, n: NodeId) -> &str {
        match self.nodes[n as usize].kind {
            Kind::Element => self.syms.get(self.tag(n)),
            _ => "",
        }
    }
    /// Text of a text node.
    pub fn text(&self, n: NodeId) -> &str {
        let node = &self.nodes[n as usize];
        if node.kind != Kind::Text {
            return "";
        }
        &self.text[node.a as usize..(node.a + node.b) as usize]
    }
    #[inline]
    pub fn parent(&self, n: NodeId) -> Option<NodeId> {
        let p = self.nodes[n as usize].parent;
        (p != NONE).then_some(p)
    }
    #[inline]
    pub fn first_child(&self, n: NodeId) -> Option<NodeId> {
        let c = self.nodes[n as usize].first;
        (c != NONE).then_some(c)
    }
    #[inline]
    pub fn last_child(&self, n: NodeId) -> Option<NodeId> {
        let c = self.nodes[n as usize].last;
        (c != NONE).then_some(c)
    }
    #[inline]
    pub fn next_sibling(&self, n: NodeId) -> Option<NodeId> {
        let c = self.nodes[n as usize].next;
        (c != NONE).then_some(c)
    }
    #[inline]
    pub fn prev_sibling(&self, n: NodeId) -> Option<NodeId> {
        let c = self.nodes[n as usize].prev;
        (c != NONE).then_some(c)
    }
    pub fn children(&self, n: NodeId) -> Children<'_> {
        Children { doc: self, next: self.nodes[n as usize].first }
    }
    pub fn element_children(&self, n: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        self.children(n).filter(move |&c| self.is_element(c))
    }
    /// First element child with this tag.
    pub fn child_named(&self, n: NodeId, tag: Sym) -> Option<NodeId> {
        self.children(n).find(|&c| self.is_element(c) && self.tag(c) == tag)
    }
    pub fn child_named_str(&self, n: NodeId, tag: &str) -> Option<NodeId> {
        let s = self.syms.lookup(tag)?;
        self.child_named(n, s)
    }
    pub fn has_element_children(&self, n: NodeId) -> bool {
        self.element_children(n).next().is_some()
    }
    /// Every descendant in document order (the node itself excluded).
    pub fn descendants(&self, n: NodeId) -> Descendants<'_> {
        Descendants { doc: self, root: n, next: self.nodes[n as usize].first }
    }
    /// Number of ancestors (the document node has depth 0).
    pub fn depth(&self, mut n: NodeId) -> usize {
        let mut d = 0;
        while let Some(p) = self.parent(n) {
            d += 1;
            n = p;
        }
        d
    }
    pub fn is_ancestor(&self, ancestor: NodeId, mut n: NodeId) -> bool {
        while let Some(p) = self.parent(n) {
            if p == ancestor {
                return true;
            }
            n = p;
        }
        false
    }
    /// XPath string-value: text of a text node, the concatenated descendant text of an element.
    pub fn string_value(&self, n: NodeId) -> std::borrow::Cow<'_, str> {
        match self.kind(n) {
            Kind::Text => std::borrow::Cow::Borrowed(self.text(n)),
            _ => {
                let mut parts = self.descendants(n).filter(|&d| self.is_text(d));
                match (parts.next(), parts.next()) {
                    (None, _) => std::borrow::Cow::Borrowed(""),
                    (Some(one), None) => std::borrow::Cow::Borrowed(self.text(one)),
                    (Some(one), Some(two)) => {
                        let mut s = String::from(self.text(one));
                        s.push_str(self.text(two));
                        for p in parts {
                            s.push_str(self.text(p));
                        }
                        std::borrow::Cow::Owned(s)
                    }
                }
            }
        }
    }
    /// Trimmed text of an element with no element children, the way values are read.
    pub fn value(&self, n: NodeId) -> Option<String> {
        if self.kind(n) != Kind::Element || self.has_element_children(n) {
            return None;
        }
        Some(self.string_value(n).trim().to_string())
    }

    // ----- attributes -----------------------------------------------------------------------

    pub fn attrs(&self, n: NodeId) -> Attrs<'_> {
        let first = if self.is_element(n) { self.nodes[n as usize].b } else { NONE };
        Attrs { doc: self, next: first }
    }
    pub fn attr_value_at(&self, index: u32) -> &str {
        let a = &self.attrs[index as usize];
        &self.text[a.start as usize..(a.start + a.len) as usize]
    }
    pub fn attr_name_at(&self, index: u32) -> Sym {
        self.attrs[index as usize].name
    }
    pub fn attr_index(&self, n: NodeId, name: Sym) -> Option<u32> {
        self.attrs(n).find(|&i| self.attrs[i as usize].name == name)
    }
    pub fn attr(&self, n: NodeId, name: Sym) -> Option<&str> {
        self.attr_index(n, name).map(|i| self.attr_value_at(i))
    }
    pub fn attr_str(&self, n: NodeId, name: &str) -> Option<&str> {
        let s = self.syms.lookup(name)?;
        self.attr(n, s)
    }
    /// Set (or add) an attribute; returns whether it was new.
    pub fn set_attr(&mut self, n: NodeId, name: Sym, value: &str) -> bool {
        debug_assert!(self.is_element(n));
        if let Some(i) = self.attr_index(n, name) {
            let (start, len) = self.store_text(value);
            let a = &mut self.attrs[i as usize];
            a.start = start;
            a.len = len;
            self.touch_attr(n, name);
            return false;
        }
        let (start, len) = self.store_text(value);
        let idx = self.attrs.len() as u32;
        self.attrs.push(Attr { name, start, len, next: NONE });
        // append at the end, keeping document order of attributes
        let mut cur = self.nodes[n as usize].b;
        if cur == NONE {
            self.nodes[n as usize].b = idx;
        } else {
            while self.attrs[cur as usize].next != NONE {
                cur = self.attrs[cur as usize].next;
            }
            self.attrs[cur as usize].next = idx;
        }
        self.touch_attr(n, name);
        true
    }
    pub fn remove_attr(&mut self, n: NodeId, name: Sym) -> bool {
        let mut prev = NONE;
        let mut cur = self.nodes[n as usize].b;
        while cur != NONE {
            let next = self.attrs[cur as usize].next;
            if self.attrs[cur as usize].name == name {
                if prev == NONE {
                    self.nodes[n as usize].b = next;
                } else {
                    self.attrs[prev as usize].next = next;
                }
                self.touch_attr(n, name);
                return true;
            }
            prev = cur;
            cur = next;
        }
        false
    }

    // ----- structure ------------------------------------------------------------------------

    fn link_last(&mut self, parent: NodeId, child: NodeId) {
        let last = self.nodes[parent as usize].last;
        {
            let c = &mut self.nodes[child as usize];
            c.parent = parent;
            c.prev = last;
            c.next = NONE;
        }
        if last == NONE {
            self.nodes[parent as usize].first = child;
        } else {
            self.nodes[last as usize].next = child;
        }
        self.nodes[parent as usize].last = child;
    }

    /// Unlink a node from its parent (it stays alive; use `remove` to kill it).
    pub fn detach(&mut self, n: NodeId) {
        let (parent, prev, next) = {
            let node = &self.nodes[n as usize];
            (node.parent, node.prev, node.next)
        };
        if parent == NONE {
            return;
        }
        self.touch(n);
        if prev == NONE {
            self.nodes[parent as usize].first = next;
        } else {
            self.nodes[prev as usize].next = next;
        }
        if next == NONE {
            self.nodes[parent as usize].last = prev;
        } else {
            self.nodes[next as usize].prev = prev;
        }
        let node = &mut self.nodes[n as usize];
        node.parent = NONE;
        node.prev = NONE;
        node.next = NONE;
    }

    /// Detach and mark dead, the subtree included.
    pub fn remove(&mut self, n: NodeId) {
        self.detach(n);
        self.nodes[n as usize].dead = true;
        let mut stack: Vec<NodeId> = self.children(n).collect();
        while let Some(c) = stack.pop() {
            self.nodes[c as usize].dead = true;
            stack.extend(self.children(c));
        }
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.detach(child);
        self.link_last(parent, child);
        self.touch(child);
    }

    pub fn prepend_child(&mut self, parent: NodeId, child: NodeId) {
        match self.first_child(parent) {
            Some(first) => self.insert_before(first, child),
            None => self.append_child(parent, child),
        }
    }

    pub fn insert_before(&mut self, reference: NodeId, new: NodeId) {
        self.detach(new);
        let parent = self.nodes[reference as usize].parent;
        debug_assert!(parent != NONE);
        let prev = self.nodes[reference as usize].prev;
        {
            let c = &mut self.nodes[new as usize];
            c.parent = parent;
            c.prev = prev;
            c.next = reference;
        }
        self.nodes[reference as usize].prev = new;
        if prev == NONE {
            self.nodes[parent as usize].first = new;
        } else {
            self.nodes[prev as usize].next = new;
        }
        self.touch(new);
    }

    pub fn insert_after(&mut self, reference: NodeId, new: NodeId) {
        match self.next_sibling(reference) {
            Some(next) => self.insert_before(next, new),
            None => {
                let parent = self.nodes[reference as usize].parent;
                self.append_child(parent, new);
            }
        }
    }

    /// Deep copy of a subtree, detached. `origin` replaces every copied node's origin when given
    /// (a patch value being applied); `via` is stamped on every copy (inheritance).
    pub fn clone_subtree(&mut self, n: NodeId, origin: Option<u32>, via: u32) -> NodeId {
        let copy = self.clone_one(n, origin, via);
        let mut stack: Vec<(NodeId, NodeId)> = vec![(n, copy)];
        while let Some((src, dst)) = stack.pop() {
            let kids: Vec<NodeId> = self.children(src).collect();
            for k in kids {
                let kc = self.clone_one(k, origin, via);
                self.link_last(dst, kc);
                stack.push((k, kc));
            }
        }
        copy
    }

    fn clone_one(&mut self, n: NodeId, origin: Option<u32>, via: u32) -> NodeId {
        let src = self.nodes[n as usize].clone();
        let origin = origin.unwrap_or(src.origin);
        let via = if via != 0 { via } else { src.via };
        match src.kind {
            Kind::Text => {
                let id = self.new_text(&self.text[src.a as usize..(src.a + src.b) as usize].to_string(), origin);
                self.nodes[id as usize].via = via;
                id
            }
            _ => {
                let id = self.new_element(Sym(src.a), origin);
                self.nodes[id as usize].via = via;
                // copy attributes in order
                let attrs: Vec<(Sym, String)> = self.attrs(n).map(|i| (self.attr_name_at(i), self.attr_value_at(i).to_string())).collect();
                for (name, value) in attrs {
                    self.set_attr(id, name, &value);
                }
                id
            }
        }
    }

    /// `.NET InnerText = text`: every child goes, one text node remains.
    pub fn set_inner_text(&mut self, n: NodeId, text: &str, origin: u32) {
        let kids: Vec<NodeId> = self.children(n).collect();
        for k in kids {
            self.remove(k);
        }
        let t = self.new_text(text, origin);
        self.append_child(n, t);
    }

    /// Replace an element's tag (PatchOperationSetName). .NET makes a new element; the id is kept
    /// here so journal entries stay valid.
    pub fn rename(&mut self, n: NodeId, name: Sym) {
        debug_assert!(self.is_element(n));
        self.touch(n);
        self.nodes[n as usize].a = name.0;
        self.touch(n);
    }

    // ----- the root index -------------------------------------------------------------------

    fn is_root_child(&self, n: NodeId) -> bool {
        match self.parent(n) {
            Some(p) => self.parent(p) == Some(DOCUMENT),
            None => false,
        }
    }

    /// Called with a node whose position, text or tag is about to change or has changed.
    fn touch(&mut self, n: NodeId) {
        if self.root_index.borrow().is_none() {
            return;
        }
        // A def itself, its defName child, or the text under it.
        let hit = self.is_root_child(n) || self.parent(n).map(|p| self.is_root_child(p)).unwrap_or(false) || self.parent(n).and_then(|p| self.parent(p)).map(|g| self.is_root_child(g)).unwrap_or(false);
        if hit {
            *self.root_index.borrow_mut() = None;
        }
    }

    fn touch_attr(&mut self, n: NodeId, name: Sym) {
        if name == self.s.name && self.is_root_child(n) {
            *self.root_index.borrow_mut() = None;
        }
    }

    fn build_root_index(&self) -> RootIndex {
        let mut ix = RootIndex::default();
        if let Some(root) = self.root() {
            for d in self.element_children(root) {
                let tag = self.tag(d);
                if let Some(dn) = self.child_named(d, self.s.def_name) {
                    let key: Box<str> = self.string_value(dn).as_ref().into();
                    ix.by_def_name.entry((tag, key)).or_default().push(d);
                }
                if let Some(name) = self.attr(d, self.s.name) {
                    ix.by_name.entry((tag, name.into())).or_default().push(d);
                }
            }
        }
        ix
    }

    fn with_index<T>(&self, f: impl FnOnce(&RootIndex) -> T) -> T {
        if self.root_index.borrow().is_none() {
            *self.root_index.borrow_mut() = Some(self.build_root_index());
        }
        let b = self.root_index.borrow();
        f(b.as_ref().unwrap())
    }

    /// Root children with this tag whose `defName` string-value equals `def_name` (document order).
    pub fn defs_named(&self, tag: Sym, def_name: &str) -> Vec<NodeId> {
        self.with_index(|ix| ix.by_def_name.get(&(tag, def_name.into())).cloned().unwrap_or_default())
    }
    /// Root children with this tag whose `Name` attribute equals `name`.
    pub fn defs_with_name(&self, tag: Sym, name: &str) -> Vec<NodeId> {
        self.with_index(|ix| ix.by_name.get(&(tag, name.into())).cloned().unwrap_or_default())
    }

    // ----- import from roxmltree ------------------------------------------------------------

    /// Copy an element (recursively) into the arena, detached. Whitespace-only text is dropped,
    /// comments and processing instructions too, as .NET's XmlDocument does for RimWorld.
    pub fn import(&mut self, src: roxmltree::Node, origin: u32) -> NodeId {
        let id = self.new_element_named(src.tag_name().name(), origin);
        for a in src.attributes() {
            let name = self.syms.intern(a.name());
            self.set_attr(id, name, a.value());
        }
        for c in src.children() {
            if c.is_element() {
                let cid = self.import(c, origin);
                self.link_last(id, cid);
            } else if c.is_text() {
                let t = c.text().unwrap_or("");
                if !t.trim().is_empty() {
                    let tid = self.new_text(t, origin);
                    self.link_last(id, tid);
                }
            }
        }
        id
    }

    // ----- output ---------------------------------------------------------------------------

    /// The path of `n` inside `root`, RimWorld style: `statBases/MaxHitPoints`, `comps/li[2]`.
    /// Index suffixes only on `li` and on repeated siblings.
    pub fn path_within(&self, root: NodeId, n: NodeId) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut cur = n;
        while cur != root {
            let Some(p) = self.parent(cur) else { break };
            match self.kind(cur) {
                Kind::Text => parts.push("text()".into()),
                Kind::Element => {
                    let tag = self.tag(cur);
                    let same: Vec<NodeId> = self.children(p).filter(|&c| self.is_element(c) && self.tag(c) == tag).collect();
                    if same.len() > 1 {
                        let pos = same.iter().position(|&c| c == cur).unwrap_or(0) + 1;
                        parts.push(format!("{}[{}]", self.syms.get(tag), pos));
                    } else {
                        parts.push(self.syms.get(tag).to_string());
                    }
                }
                Kind::Document => {}
            }
            cur = p;
        }
        parts.reverse();
        parts.join("/")
    }

    /// Serialize a subtree as indented XML (for the inspector and for tests).
    pub fn to_xml(&self, n: NodeId) -> String {
        let mut out = String::new();
        self.write(n, 0, &mut out);
        out
    }

    fn write(&self, n: NodeId, depth: usize, out: &mut String) {
        match self.kind(n) {
            Kind::Document => {
                for c in self.children(n) {
                    self.write(c, depth, out);
                }
            }
            Kind::Text => {
                out.push_str(&crate::xmlutil::escape(self.text(n)));
            }
            Kind::Element => {
                let pad = "  ".repeat(depth);
                out.push_str(&pad);
                out.push('<');
                out.push_str(self.name(n));
                for i in self.attrs(n) {
                    out.push(' ');
                    out.push_str(self.syms.get(self.attr_name_at(i)));
                    out.push_str("=\"");
                    out.push_str(&crate::xmlutil::escape(self.attr_value_at(i)));
                    out.push('"');
                }
                let kids: Vec<NodeId> = self.children(n).collect();
                if kids.is_empty() {
                    out.push_str(" />\n");
                } else if kids.iter().all(|&k| self.is_text(k)) {
                    out.push('>');
                    for k in kids {
                        out.push_str(&crate::xmlutil::escape(self.text(k)));
                    }
                    out.push_str("</");
                    out.push_str(self.name(n));
                    out.push_str(">\n");
                } else {
                    out.push_str(">\n");
                    for k in kids {
                        if self.is_text(k) {
                            out.push_str(&"  ".repeat(depth + 1));
                            out.push_str(&crate::xmlutil::escape(self.text(k)));
                            out.push('\n');
                        } else {
                            self.write(k, depth + 1, out);
                        }
                    }
                    out.push_str(&pad);
                    out.push_str("</");
                    out.push_str(self.name(n));
                    out.push_str(">\n");
                }
            }
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

pub struct Children<'a> {
    doc: &'a Doc,
    next: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        self.next = self.doc.nodes[id as usize].next;
        Some(id)
    }
}

pub struct Descendants<'a> {
    doc: &'a Doc,
    root: NodeId,
    next: u32,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        let node = &self.doc.nodes[id as usize];
        // pre-order: first child, else next sibling, else climb until a next sibling appears
        if node.first != NONE {
            self.next = node.first;
        } else {
            let mut cur = id;
            loop {
                if cur == self.root {
                    self.next = NONE;
                    break;
                }
                let n = &self.doc.nodes[cur as usize];
                if n.next != NONE {
                    self.next = n.next;
                    break;
                }
                if n.parent == NONE || n.parent == self.root {
                    self.next = NONE;
                    break;
                }
                cur = n.parent;
            }
        }
        Some(id)
    }
}

pub struct Attrs<'a> {
    doc: &'a Doc,
    next: u32,
}

impl Iterator for Attrs<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.next == NONE {
            return None;
        }
        let i = self.next;
        self.next = self.doc.attrs[i as usize].next;
        Some(i)
    }
}

/// Parse an XML text and import the root element's element children under `parent`
/// (the unified `Defs` root, or a patch file's holder). Returns the imported nodes.
pub fn import_children(doc: &mut Doc, xml: &str, parent: NodeId, origin: u32) -> Result<Vec<NodeId>, roxmltree::Error> {
    let parsed = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX })?;
    let mut out = Vec::new();
    for c in parsed.root_element().children() {
        if c.is_element() {
            let id = doc.import(c, origin);
            doc.append_child(parent, id);
            out.push(id);
        }
    }
    Ok(out)
}

/// The tag of a parsed document's root element, for the `Defs` / `Patch` checks.
pub fn root_tag(xml: &str) -> Result<String, roxmltree::Error> {
    let parsed = roxmltree::Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: true, nodes_limit: u32::MAX })?;
    Ok(parsed.root_element().tag_name().name().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with(xml: &str) -> (Doc, NodeId) {
        let mut d = Doc::new();
        let root = d.new_element_named("Defs", 0);
        d.append_child(DOCUMENT, root);
        import_children(&mut d, xml, root, 1).unwrap();
        (d, root)
    }

    #[test]
    fn imports_and_serializes() {
        let (d, root) = doc_with("<Defs><ThingDef Name=\"Base\" Abstract=\"True\"><defName>Wall</defName><statBases><MaxHitPoints>100</MaxHitPoints></statBases><comps><li Class=\"CompX\"/><li>two</li></comps></ThingDef></Defs>");
        let def = d.element_children(root).next().unwrap();
        assert_eq!(d.name(def), "ThingDef");
        assert_eq!(d.attr_str(def, "Name"), Some("Base"));
        let dn = d.child_named_str(def, "defName").unwrap();
        assert_eq!(d.value(dn).as_deref(), Some("Wall"));
        let comps = d.child_named_str(def, "comps").unwrap();
        let second = d.element_children(comps).nth(1).unwrap();
        assert_eq!(d.path_within(def, second), "comps/li[2]");
        assert_eq!(d.path_within(def, d.child_named_str(d.child_named_str(def, "statBases").unwrap(), "MaxHitPoints").unwrap()), "statBases/MaxHitPoints");
        let xml = d.to_xml(def);
        assert!(xml.contains("<ThingDef Name=\"Base\" Abstract=\"True\">"), "{xml}");
        assert!(xml.contains("<li Class=\"CompX\" />"));
        assert_eq!(d.descendants(def).count(), 10);
    }

    #[test]
    fn mutations_keep_links_straight() {
        let (mut d, root) = doc_with("<Defs><A><defName>a</defName></A><B><defName>b</defName></B></Defs>");
        let a = d.element_children(root).next().unwrap();
        let b = d.element_children(root).nth(1).unwrap();
        let c = d.new_element_named("C", 2);
        d.insert_before(b, c);
        let names: Vec<&str> = d.element_children(root).map(|n| d.name(n)).collect();
        assert_eq!(names, ["A", "C", "B"]);
        d.prepend_child(root, d.new_element_named("Z", 2));
        let names: Vec<&str> = d.element_children(root).map(|n| d.name(n)).collect();
        assert_eq!(names, ["Z", "A", "C", "B"]);
        d.remove(a);
        assert!(d.nodes[a as usize].dead);
        let names: Vec<&str> = d.element_children(root).map(|n| d.name(n)).collect();
        assert_eq!(names, ["Z", "C", "B"]);
        d.insert_after(c, d.new_element_named("D", 2));
        let names: Vec<&str> = d.element_children(root).map(|n| d.name(n)).collect();
        assert_eq!(names, ["Z", "C", "D", "B"]);
        assert_eq!(d.last_child(root).map(|n| d.name(n)), Some("B"));
        assert_eq!(d.prev_sibling(b).map(|n| d.name(n)), Some("D"));
    }

    #[test]
    fn root_index_follows_changes() {
        let (mut d, root) = doc_with("<Defs><ThingDef><defName>Wall</defName></ThingDef><ThingDef Name=\"WallBase\"><defName>Wall</defName></ThingDef></Defs>");
        let tag = d.syms.lookup("ThingDef").unwrap();
        assert_eq!(d.defs_named(tag, "Wall").len(), 2);
        assert_eq!(d.defs_with_name(tag, "WallBase").len(), 1);
        let second = d.element_children(root).nth(1).unwrap();
        d.remove(second);
        assert_eq!(d.defs_named(tag, "Wall").len(), 1);
        // a text change under defName is noticed
        let first = d.element_children(root).next().unwrap();
        let dn = d.child_named_str(first, "defName").unwrap();
        d.set_inner_text(dn, "Door", 3);
        assert!(d.defs_named(tag, "Wall").is_empty());
        assert_eq!(d.defs_named(tag, "Door").len(), 1);
        // a whole def appended through the api
        let nd = d.new_element_named("ThingDef", 3);
        let ndn = d.new_element_named("defName", 3);
        let t = d.new_text("Bed", 3);
        d.append_child(ndn, t);
        d.append_child(nd, ndn);
        d.append_child(root, nd);
        assert_eq!(d.defs_named(tag, "Bed"), vec![nd]);
        d.set_attr(nd, d.s.name, "BedBase");
        assert_eq!(d.defs_with_name(tag, "BedBase"), vec![nd]);
    }

    #[test]
    fn clone_and_attributes() {
        let (mut d, root) = doc_with("<Defs><A x=\"1\" y=\"2\"><b>t</b></A></Defs>");
        let a = d.element_children(root).next().unwrap();
        let copy = d.clone_subtree(a, Some(9), 4);
        assert_eq!(d.attr_str(copy, "y"), Some("2"));
        assert_eq!(d.nodes[copy as usize].origin, 9);
        let b = d.element_children(copy).next().unwrap();
        assert_eq!(d.nodes[b as usize].via, 4);
        assert_eq!(d.value(b).as_deref(), Some("t"));
        assert!(d.set_attr(copy, d.syms.intern("z"), "3"));
        assert!(!d.set_attr(copy, d.syms.intern("x"), "10"));
        assert_eq!(d.attr_str(copy, "x"), Some("10"));
        assert!(d.remove_attr(copy, d.syms.intern("y")));
        let names: Vec<&str> = d.attrs(copy).map(|i| d.syms.get(d.attr_name_at(i))).collect();
        assert_eq!(names, ["x", "z"]);
        assert_eq!(d.string_value(a), "t");
    }
}
