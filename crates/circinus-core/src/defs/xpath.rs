//! XPath 1.0 over the def arena.
//!
//! RimWorld hands every `PatchOperation`'s `xpath` to .NET's `XmlDocument.SelectNodes`, so a
//! patch means whatever XPath 1.0 says it means — not whatever subset would be convenient. This
//! is that language: all thirteen axes, predicates with real position semantics, the core
//! function library, and XPath's conversion rules between node-sets, strings, numbers and
//! booleans. An expression compiles once and is evaluated many times.
//!
//! Two things keep it quick on a document with millions of nodes. Results carry an `ordered`
//! flag, so the usual forward path (`Defs/ThingDef[…]/statBases`) never needs a document-order
//! sort; and `Defs/TypeName[defName="X"]` — the shape of nearly every patch in the wild — is
//! answered from the arena's root index instead of a scan over a hundred thousand siblings.

use super::tree::{Doc, Kind, NodeId, Sym, DOCUMENT};
use std::collections::HashSet;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XPathError(pub String);

impl std::fmt::Display for XPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for XPathError {}

type R<T> = std::result::Result<T, XPathError>;

fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(XPathError(msg.into()))
}

/// One thing a node-set can hold. The arena has no attribute nodes of its own, so an attribute
/// is named by its element and its index in that element's attribute list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    Node(NodeId),
    Attr { node: NodeId, index: u32 },
}

impl Item {
    /// The element an item belongs to: itself, or the owner of an attribute.
    pub fn owner(self) -> NodeId {
        match self {
            Item::Node(n) => n,
            Item::Attr { node, .. } => node,
        }
    }
    pub fn node(self) -> Option<NodeId> {
        match self {
            Item::Node(n) => Some(n),
            Item::Attr { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nodes(Vec<Item>),
    String(String),
    Number(f64),
    Bool(bool),
}

// ---------------------------------------------------------------------------------------------
// The compiled form
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Child,
    Descendant,
    DescendantOrSelf,
    Parent,
    Ancestor,
    AncestorOrSelf,
    Zelf,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
    Attribute,
    /// No namespaces in RimWorld's XML; the axis exists and is always empty.
    Namespace,
}

impl Axis {
    /// Reverse axes number their nodes away from the context node, which is what a positional
    /// predicate counts (`preceding-sibling::li[1]` is the nearest one).
    fn is_reverse(self) -> bool {
        matches!(self, Axis::Parent | Axis::Ancestor | Axis::AncestorOrSelf | Axis::PrecedingSibling | Axis::Preceding)
    }
    /// Forward axes that can never return an ancestor of another result, so a walk over an
    /// ordered input stays ordered.
    fn keeps_order(self) -> bool {
        matches!(self, Axis::Child | Axis::Attribute | Axis::Zelf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeTest {
    Name(String),
    Any,
    AnyNode,
    Text,
    /// `comment()` and `processing-instruction()`: the arena keeps neither, so always empty.
    Never,
}

#[derive(Debug, Clone)]
struct Step {
    axis: Axis,
    test: NodeTest,
    preds: Vec<Ast>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Union,
}

#[derive(Debug, Clone)]
enum Ast {
    /// A location path. `absolute` starts at the document node.
    Path { absolute: bool, steps: Vec<Step> },
    /// A primary expression with predicates, optionally followed by more steps: `(//li)[1]/x`.
    Filter { base: Box<Ast>, preds: Vec<Ast>, steps: Vec<Step> },
    Binary { op: Op, lhs: Box<Ast>, rhs: Box<Ast> },
    Neg(Box<Ast>),
    Literal(String),
    Number(f64),
    Call { name: String, args: Vec<Ast> },
}

/// A compiled expression. Compile once, evaluate as often as you like.
#[derive(Debug, Clone)]
pub struct Expr {
    ast: Ast,
    text: String,
}

impl Expr {
    /// The expression as it was written, for error messages and for the UI.
    pub fn text(&self) -> &str {
        &self.text
    }
}

// ---------------------------------------------------------------------------------------------
// Lexing
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Literal(String),
    Number(f64),
    /// `name(` — a function call or a node-type test; the lexer marks it so `div` as a name and
    /// `div` as an operator can be told apart the way XPath requires.
    FnName(String),
    /// `axis::`
    AxisName(String),
    Op(&'static str),
    Var(String),
}

fn is_name_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn lex(src: &str) -> R<Vec<Tok>> {
    let cs: Vec<char> = src.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // A name is an operator (and, or, div, mod) only where an operator can follow; XPath
        // decides that from the previous token.
        let operator_expected = matches!(out.last(), Some(Tok::Name(_)) | Some(Tok::Literal(_)) | Some(Tok::Number(_)) | Some(Tok::Var(_)) | Some(Tok::Op(")")) | Some(Tok::Op("]")) | Some(Tok::Op("*")));
        match c {
            '"' | '\'' => {
                let quote = c;
                i += 1;
                let start = i;
                while i < cs.len() && cs[i] != quote {
                    i += 1;
                }
                if i >= cs.len() {
                    return err("a string was opened but never closed");
                }
                out.push(Tok::Literal(cs[start..i].iter().collect()));
                i += 1;
            }
            '0'..='9' => {
                let start = i;
                while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                    i += 1;
                }
                let text: String = cs[start..i].iter().collect();
                match text.parse::<f64>() {
                    Ok(n) => out.push(Tok::Number(n)),
                    Err(_) => return err(format!("`{text}` is not a number")),
                }
            }
            '.' if i + 1 < cs.len() && cs[i + 1].is_ascii_digit() => {
                let start = i;
                i += 1;
                while i < cs.len() && cs[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = cs[start..i].iter().collect();
                out.push(Tok::Number(text.parse::<f64>().map_err(|_| XPathError(format!("`{text}` is not a number")))?));
            }
            '$' => {
                i += 1;
                let start = i;
                while i < cs.len() && is_name_char(cs[i]) {
                    i += 1;
                }
                out.push(Tok::Var(cs[start..i].iter().collect()));
            }
            '/' if i + 1 < cs.len() && cs[i + 1] == '/' => {
                out.push(Tok::Op("//"));
                i += 2;
            }
            '.' if i + 1 < cs.len() && cs[i + 1] == '.' => {
                out.push(Tok::Op(".."));
                i += 2;
            }
            '!' if i + 1 < cs.len() && cs[i + 1] == '=' => {
                out.push(Tok::Op("!="));
                i += 2;
            }
            '<' | '>' if i + 1 < cs.len() && cs[i + 1] == '=' => {
                out.push(Tok::Op(if c == '<' { "<=" } else { ">=" }));
                i += 2;
            }
            '/' | '.' | '@' | '(' | ')' | '[' | ']' | ',' | '+' | '-' | '=' | '<' | '>' | '|' => {
                let s: &'static str = match c {
                    '/' => "/",
                    '.' => ".",
                    '@' => "@",
                    '(' => "(",
                    ')' => ")",
                    '[' => "[",
                    ']' => "]",
                    ',' => ",",
                    '+' => "+",
                    '-' => "-",
                    '=' => "=",
                    '<' => "<",
                    '>' => ">",
                    _ => "|",
                };
                out.push(Tok::Op(s));
                i += 1;
            }
            '*' => {
                out.push(if operator_expected { Tok::Op("*mul") } else { Tok::Op("*") });
                i += 1;
            }
            c if is_name_start(c) => {
                let start = i;
                while i < cs.len() && is_name_char(cs[i]) {
                    i += 1;
                }
                let name: String = cs[start..i].iter().collect();
                if operator_expected {
                    match name.as_str() {
                        "and" => out.push(Tok::Op("and")),
                        "or" => out.push(Tok::Op("or")),
                        "div" => out.push(Tok::Op("div")),
                        "mod" => out.push(Tok::Op("mod")),
                        _ => return err(format!("`{name}` cannot be used as an operator here")),
                    }
                    continue;
                }
                // look ahead past spaces for `::` or `(`
                let mut j = i;
                while j < cs.len() && cs[j].is_whitespace() {
                    j += 1;
                }
                if j + 1 < cs.len() && cs[j] == ':' && cs[j + 1] == ':' {
                    out.push(Tok::AxisName(name));
                    i = j + 2;
                } else if j < cs.len() && cs[j] == ':' {
                    return err(format!("`{name}:…` — namespaces are not used in RimWorld XML"));
                } else if j < cs.len() && cs[j] == '(' {
                    out.push(Tok::FnName(name));
                    i = j + 1;
                    out.push(Tok::Op("("));
                } else {
                    out.push(Tok::Name(name));
                }
            }
            other => return err(format!("`{other}` has no meaning in an XPath expression")),
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------------------------

struct Parser {
    toks: Vec<Tok>,
    at: usize,
}

/// Every function XPath 1.0 defines, with the argument counts it accepts (min, max; `None` = any).
const FUNCTIONS: &[(&str, usize, Option<usize>)] = &[
    ("last", 0, Some(0)),
    ("position", 0, Some(0)),
    ("count", 1, Some(1)),
    ("id", 1, Some(1)),
    ("local-name", 0, Some(1)),
    ("namespace-uri", 0, Some(1)),
    ("name", 0, Some(1)),
    ("string", 0, Some(1)),
    ("concat", 2, None),
    ("starts-with", 2, Some(2)),
    ("contains", 2, Some(2)),
    ("substring-before", 2, Some(2)),
    ("substring-after", 2, Some(2)),
    ("substring", 2, Some(3)),
    ("string-length", 0, Some(1)),
    ("normalize-space", 0, Some(1)),
    ("translate", 3, Some(3)),
    ("boolean", 1, Some(1)),
    ("not", 1, Some(1)),
    ("true", 0, Some(0)),
    ("false", 0, Some(0)),
    ("lang", 1, Some(1)),
    ("number", 0, Some(1)),
    ("sum", 1, Some(1)),
    ("floor", 1, Some(1)),
    ("ceiling", 1, Some(1)),
    ("round", 1, Some(1)),
];

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            return true;
        }
        false
    }
    fn expect_op(&mut self, op: &str) -> R<()> {
        if self.eat_op(op) {
            return Ok(());
        }
        err(format!("expected `{op}`"))
    }

    fn parse(&mut self) -> R<Ast> {
        let e = self.expr()?;
        if self.at != self.toks.len() {
            return err("there is more text after the end of the expression");
        }
        Ok(e)
    }

    fn expr(&mut self) -> R<Ast> {
        self.binary(0)
    }

    /// Precedence climbing: or < and < equality < relational < additive < multiplicative < union.
    fn binary(&mut self, level: usize) -> R<Ast> {
        const LEVELS: &[&[(&str, Op)]] = &[
            &[("or", Op::Or)],
            &[("and", Op::And)],
            &[("=", Op::Eq), ("!=", Op::Ne)],
            &[("<", Op::Lt), ("<=", Op::Le), (">", Op::Gt), (">=", Op::Ge)],
            &[("+", Op::Add), ("-", Op::Sub)],
            &[("*mul", Op::Mul), ("div", Op::Div), ("mod", Op::Mod)],
            &[("|", Op::Union)],
        ];
        if level >= LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(level + 1)?;
        while let Some(Tok::Op(o)) = self.peek() {
            let Some(&(_, op)) = LEVELS[level].iter().find(|(name, _)| name == o) else { break };
            self.at += 1;
            let rhs = self.binary(level + 1)?;
            lhs = Ast::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> R<Ast> {
        if self.eat_op("-") {
            return Ok(Ast::Neg(Box::new(self.unary()?)));
        }
        self.path()
    }

    /// A location path, or a primary expression followed by predicates and steps.
    fn path(&mut self) -> R<Ast> {
        // A primary expression starts a filter expression: literal, number, ( … ), function call.
        let primary = match self.peek() {
            Some(Tok::Literal(s)) => {
                let s = s.clone();
                self.at += 1;
                Some(Ast::Literal(s))
            }
            Some(Tok::Number(n)) => {
                let n = *n;
                self.at += 1;
                Some(Ast::Number(n))
            }
            Some(Tok::Var(v)) => return err(format!("`${v}` — variables are not available in a patch")),
            Some(Tok::Op("(")) => {
                self.at += 1;
                let e = self.expr()?;
                self.expect_op(")")?;
                Some(e)
            }
            // `text()`, `node()` and friends look like calls but are node tests, and a step may
            // begin with one: `label[text()="wall"]`.
            Some(Tok::FnName(name)) if !matches!(name.as_str(), "node" | "text" | "comment" | "processing-instruction") => {
                let name = name.clone();
                self.at += 1;
                self.expect_op("(")?;
                let mut args = Vec::new();
                if !self.eat_op(")") {
                    loop {
                        args.push(self.expr()?);
                        if self.eat_op(")") {
                            break;
                        }
                        self.expect_op(",")?;
                    }
                }
                match FUNCTIONS.iter().find(|(f, _, _)| *f == name) {
                    None => return err(format!("there is no function called `{name}()` in XPath 1.0")),
                    Some((_, min, max)) => {
                        if args.len() < *min || max.map(|m| args.len() > m).unwrap_or(false) {
                            return err(format!("`{name}()` does not take {} argument{}", args.len(), if args.len() == 1 { "" } else { "s" }));
                        }
                    }
                }
                Some(Ast::Call { name, args })
            }
            _ => None,
        };

        if let Some(base) = primary {
            let mut preds = Vec::new();
            while self.eat_op("[") {
                preds.push(self.expr()?);
                self.expect_op("]")?;
            }
            let mut steps = Vec::new();
            loop {
                if self.eat_op("//") {
                    steps.push(Step { axis: Axis::DescendantOrSelf, test: NodeTest::AnyNode, preds: Vec::new() });
                    steps.push(self.step()?);
                } else if self.eat_op("/") {
                    steps.push(self.step()?);
                } else {
                    break;
                }
            }
            return Ok(Ast::Filter { base: Box::new(base), preds, steps });
        }

        // A location path.
        let mut absolute = false;
        let mut steps: Vec<Step> = Vec::new();
        if self.eat_op("//") {
            absolute = true;
            steps.push(Step { axis: Axis::DescendantOrSelf, test: NodeTest::AnyNode, preds: Vec::new() });
            steps.push(self.step()?);
        } else if self.eat_op("/") {
            absolute = true;
            // `/` alone is the document node.
            if self.starts_step() {
                steps.push(self.step()?);
            }
        } else {
            steps.push(self.step()?);
        }
        loop {
            if self.eat_op("//") {
                steps.push(Step { axis: Axis::DescendantOrSelf, test: NodeTest::AnyNode, preds: Vec::new() });
                steps.push(self.step()?);
            } else if self.eat_op("/") {
                steps.push(self.step()?);
            } else {
                break;
            }
        }
        Ok(Ast::Path { absolute, steps })
    }

    fn starts_step(&self) -> bool {
        matches!(self.peek(), Some(Tok::Name(_)) | Some(Tok::FnName(_)) | Some(Tok::AxisName(_)) | Some(Tok::Op("@")) | Some(Tok::Op(".")) | Some(Tok::Op("..")) | Some(Tok::Op("*")))
    }

    fn step(&mut self) -> R<Step> {
        if self.eat_op(".") {
            return Ok(Step { axis: Axis::Zelf, test: NodeTest::AnyNode, preds: self.preds()? });
        }
        if self.eat_op("..") {
            return Ok(Step { axis: Axis::Parent, test: NodeTest::AnyNode, preds: self.preds()? });
        }
        let axis = if self.eat_op("@") {
            Axis::Attribute
        } else if let Some(Tok::AxisName(name)) = self.peek() {
            let axis = match name.as_str() {
                "child" => Axis::Child,
                "descendant" => Axis::Descendant,
                "descendant-or-self" => Axis::DescendantOrSelf,
                "parent" => Axis::Parent,
                "ancestor" => Axis::Ancestor,
                "ancestor-or-self" => Axis::AncestorOrSelf,
                "self" => Axis::Zelf,
                "following-sibling" => Axis::FollowingSibling,
                "preceding-sibling" => Axis::PrecedingSibling,
                "following" => Axis::Following,
                "preceding" => Axis::Preceding,
                "attribute" => Axis::Attribute,
                "namespace" => Axis::Namespace,
                other => return err(format!("`{other}::` is not an XPath axis")),
            };
            self.at += 1;
            axis
        } else {
            Axis::Child
        };
        let test = match self.peek() {
            Some(Tok::Op("*")) => {
                self.at += 1;
                NodeTest::Any
            }
            Some(Tok::Name(n)) => {
                let n = n.clone();
                self.at += 1;
                NodeTest::Name(n)
            }
            Some(Tok::FnName(n)) => {
                let n = n.clone();
                self.at += 1;
                self.expect_op("(")?;
                // node-type tests take no arguments; processing-instruction() may take a literal
                let t = match n.as_str() {
                    "node" => NodeTest::AnyNode,
                    "text" => NodeTest::Text,
                    "comment" => NodeTest::Never,
                    "processing-instruction" => {
                        if let Some(Tok::Literal(_)) = self.peek() {
                            self.at += 1;
                        }
                        NodeTest::Never
                    }
                    other => return err(format!("`{other}()` cannot be used as a node test")),
                };
                self.expect_op(")")?;
                t
            }
            _ => return err("expected a node test"),
        };
        Ok(Step { axis, test, preds: self.preds()? })
    }

    fn preds(&mut self) -> R<Vec<Ast>> {
        let mut preds = Vec::new();
        while self.eat_op("[") {
            preds.push(self.expr()?);
            self.expect_op("]")?;
        }
        Ok(preds)
    }
}

/// Compile an expression. Errors read the way .NET's do: the expression is wrong, and here is why.
pub fn compile(text: &str) -> R<Expr> {
    if text.trim().is_empty() {
        return err("the expression is empty");
    }
    let toks = lex(text)?;
    if toks.is_empty() {
        return err("the expression is empty");
    }
    let mut p = Parser { toks, at: 0 };
    let ast = p.parse().map_err(|e| XPathError(format!("{} in `{}`", e.0, text)))?;
    Ok(Expr { ast, text: text.to_string() })
}

// ---------------------------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------------------------

/// A node-set under evaluation, with what we know about its order.
#[derive(Debug, Clone)]
struct Set {
    items: Vec<Item>,
    /// The items are in document order with no duplicates.
    ordered: bool,
}

impl Set {
    fn new(items: Vec<Item>, ordered: bool) -> Set {
        Set { items, ordered }
    }
}

struct Ctx {
    item: Item,
    position: usize,
    size: usize,
}

struct Eval<'d> {
    doc: &'d Doc,
}

impl<'d> Eval<'d> {
    // ---- axes ---------------------------------------------------------------------------

    fn matches(&self, item: Item, test: &NodeTest) -> bool {
        let d = self.doc;
        match test {
            NodeTest::Never => false,
            NodeTest::AnyNode => true,
            NodeTest::Text => matches!(item, Item::Node(n) if d.kind(n) == Kind::Text),
            // `*` and a name test select the principal node type of the axis: elements
            // everywhere but the attribute axis, where it is attributes.
            NodeTest::Any => match item {
                Item::Attr { .. } => true,
                Item::Node(n) => d.kind(n) == Kind::Element,
            },
            NodeTest::Name(name) => match item {
                Item::Attr { node, index } => d.syms.get(d.attr_name_at(index)) == name && d.is_element(node),
                Item::Node(n) => d.kind(n) == Kind::Element && d.name(n) == name,
            },
        }
    }

    fn push_matching(&self, out: &mut Vec<Item>, item: Item, test: &NodeTest) {
        if self.matches(item, test) {
            out.push(item);
        }
    }

    /// Every item on `axis` from `from`, in axis order (reverse axes count outwards).
    fn axis_items(&self, from: Item, axis: Axis, test: &NodeTest) -> Vec<Item> {
        let d = self.doc;
        let mut out = Vec::new();
        // Only the element an attribute hangs off has children, ancestors and siblings; from an
        // attribute node the tree axes behave as they do from its owner's position, except that
        // `child`/`descendant`/`attribute` are empty.
        let n = from.owner();
        let is_attr = matches!(from, Item::Attr { .. });
        match axis {
            Axis::Zelf => self.push_matching(&mut out, from, test),
            Axis::Namespace => {}
            Axis::Attribute => {
                if !is_attr {
                    for index in d.attrs(n) {
                        self.push_matching(&mut out, Item::Attr { node: n, index }, test);
                    }
                }
            }
            Axis::Child => {
                if !is_attr {
                    for c in d.children(n) {
                        self.push_matching(&mut out, Item::Node(c), test);
                    }
                }
            }
            Axis::Descendant => {
                if !is_attr {
                    for c in d.descendants(n) {
                        self.push_matching(&mut out, Item::Node(c), test);
                    }
                }
            }
            Axis::DescendantOrSelf => {
                self.push_matching(&mut out, from, test);
                if !is_attr {
                    for c in d.descendants(n) {
                        self.push_matching(&mut out, Item::Node(c), test);
                    }
                }
            }
            Axis::Parent => {
                let p = if is_attr { Some(n) } else { d.parent(n) };
                if let Some(p) = p {
                    self.push_matching(&mut out, Item::Node(p), test);
                }
            }
            Axis::Ancestor | Axis::AncestorOrSelf => {
                if axis == Axis::AncestorOrSelf {
                    self.push_matching(&mut out, from, test);
                }
                let mut cur = if is_attr { Some(n) } else { d.parent(n) };
                while let Some(c) = cur {
                    self.push_matching(&mut out, Item::Node(c), test);
                    cur = d.parent(c);
                }
            }
            Axis::FollowingSibling => {
                if !is_attr {
                    let mut cur = d.next_sibling(n);
                    while let Some(c) = cur {
                        self.push_matching(&mut out, Item::Node(c), test);
                        cur = d.next_sibling(c);
                    }
                }
            }
            Axis::PrecedingSibling => {
                if !is_attr {
                    let mut cur = d.prev_sibling(n);
                    while let Some(c) = cur {
                        self.push_matching(&mut out, Item::Node(c), test);
                        cur = d.prev_sibling(c);
                    }
                }
            }
            Axis::Following => {
                // Everything that starts after this node ends, its descendants excluded.
                let mut cur = n;
                loop {
                    let mut sib = d.next_sibling(cur);
                    while let Some(s) = sib {
                        self.push_matching(&mut out, Item::Node(s), test);
                        for x in d.descendants(s) {
                            self.push_matching(&mut out, Item::Node(x), test);
                        }
                        sib = d.next_sibling(s);
                    }
                    match d.parent(cur) {
                        Some(p) if p != DOCUMENT => cur = p,
                        _ => break,
                    }
                }
            }
            Axis::Preceding => {
                // Everything that ends before this node starts, its ancestors excluded; in
                // reverse document order, which is this axis's own order.
                let mut cur = n;
                loop {
                    let mut sib = d.prev_sibling(cur);
                    while let Some(s) = sib {
                        let mut sub: Vec<NodeId> = d.descendants(s).collect();
                        sub.reverse();
                        for x in sub {
                            self.push_matching(&mut out, Item::Node(x), test);
                        }
                        self.push_matching(&mut out, Item::Node(s), test);
                        sib = d.prev_sibling(s);
                    }
                    match d.parent(cur) {
                        Some(p) if p != DOCUMENT => cur = p,
                        _ => break,
                    }
                }
            }
        }
        out
    }

    // ---- document order -----------------------------------------------------------------

    /// The chain of child indices from the document node down to `item`, which orders any two
    /// items in the document lexicographically. Attributes sort after their element and before
    /// its children, by their position in the attribute list.
    fn order_key(&self, item: Item) -> Vec<i64> {
        let d = self.doc;
        let mut chain: Vec<i64> = Vec::new();
        let mut cur = item.owner();
        while let Some(p) = d.parent(cur) {
            let mut idx = 0i64;
            for c in d.children(p) {
                if c == cur {
                    break;
                }
                idx += 1;
            }
            chain.push(idx);
            cur = p;
        }
        chain.reverse();
        if let Item::Attr { index, .. } = item {
            // Between the element itself and its first child.
            chain.push(i64::MIN + 1 + index as i64);
        }
        chain
    }

    /// Put a set in document order and drop duplicates. Only called when a step could have
    /// disturbed the order, which the usual patch expression never does.
    fn normalize(&self, set: Set) -> Vec<Item> {
        if set.ordered {
            return set.items;
        }
        let mut seen: HashSet<Item> = HashSet::with_capacity(set.items.len());
        let mut keyed: Vec<(Vec<i64>, Item)> = Vec::with_capacity(set.items.len());
        for it in set.items {
            if seen.insert(it) {
                keyed.push((self.order_key(it), it));
            }
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        keyed.into_iter().map(|(_, it)| it).collect()
    }

    // ---- steps ---------------------------------------------------------------------------

    fn eval_step(&self, input: Set, step: &Step) -> R<Set> {
        // `Defs/ThingDef[defName="X"]` is most of the patches ever written: answer it from the
        // arena's index rather than walking a hundred thousand root children.
        if let Some(fast) = self.fast_step(&input, step)? {
            return Ok(fast);
        }
        let mut out: Vec<Item> = Vec::new();
        let ordered_in = input.ordered;
        for it in &input.items {
            let mut got = self.axis_items(*it, step.axis, &step.test);
            for (i, pred) in step.preds.iter().enumerate() {
                got = self.filter(got, pred, i == 0 && step.axis.is_reverse())?;
            }
            out.extend(got);
        }
        // One context node cannot produce duplicates; several can, and only child/attribute/self
        // steps keep an ordered input ordered.
        let ordered = ordered_in && (input.items.len() <= 1 || step.axis.keeps_order()) && !step.axis.is_reverse();
        Ok(Set::new(out, ordered || input.items.len() <= 1 && !step.axis.is_reverse()))
    }

    /// Keep the items a predicate is true for. `reverse` says positions count backwards.
    fn filter(&self, items: Vec<Item>, pred: &Ast, reverse: bool) -> R<Vec<Item>> {
        let size = items.len();
        let mut out = Vec::new();
        for (i, it) in items.into_iter().enumerate() {
            let position = if reverse { size - i } else { i + 1 };
            let ctx = Ctx { item: it, position, size };
            let v = self.eval(pred, &ctx)?;
            // A number is shorthand for position() = n.
            let keep = match v {
                Value::Number(n) => n == position as f64,
                other => self.to_bool(&other),
            };
            if keep {
                out.push(it);
            }
        }
        Ok(out)
    }

    /// `Defs/TypeName[defName="X"]` and `[@Name="X"]` answered from the root index. Returns None
    /// when the shape does not fit and the step must be walked normally.
    fn fast_step(&self, input: &Set, step: &Step) -> R<Option<Set>> {
        let d = self.doc;
        if step.axis != Axis::Child || step.preds.is_empty() {
            return Ok(None);
        }
        let NodeTest::Name(tag) = &step.test else { return Ok(None) };
        let Some(root) = d.root() else { return Ok(None) };
        if input.items.len() != 1 || input.items[0] != Item::Node(root) {
            return Ok(None);
        }
        // A positional predicate after the first counts over the whole child list, so only the
        // full walk can answer it.
        if step.preds.iter().skip(1).any(is_positional) {
            return Ok(None);
        }
        let Some(sym) = d.syms.lookup(tag) else { return Ok(Some(Set::new(Vec::new(), true))) };
        let Some(keys) = lookup_keys(&step.preds[0]) else { return Ok(None) };
        let mut candidates: Vec<NodeId> = Vec::new();
        for k in &keys {
            let found = match k {
                Key::DefName(v) => d.defs_named(sym, v),
                Key::NameAttr(v) => d.defs_with_name(sym, v),
            };
            for f in found {
                if !candidates.contains(&f) {
                    candidates.push(f);
                }
            }
        }
        if keys.len() > 1 {
            // A union of lookups is not in document order; the arena's index is per key.
            candidates.sort_by_key(|n| self.order_key(Item::Node(*n)));
        }
        // Every predicate still runs, so the meaning is exactly the walk's.
        let mut items: Vec<Item> = candidates.into_iter().map(Item::Node).collect();
        for pred in &step.preds {
            items = self.filter(items, pred, false)?;
        }
        Ok(Some(Set::new(items, true)))
    }

    // ---- expressions ----------------------------------------------------------------------

    fn eval(&self, ast: &Ast, ctx: &Ctx) -> R<Value> {
        match ast {
            Ast::Literal(s) => Ok(Value::String(s.clone())),
            Ast::Number(n) => Ok(Value::Number(*n)),
            Ast::Neg(e) => {
                let v = self.eval(e, ctx)?;
                Ok(Value::Number(-self.to_number(&v)))
            }
            Ast::Path { absolute, steps } => {
                let start = if *absolute { Item::Node(DOCUMENT) } else { ctx.item };
                let mut set = Set::new(vec![start], true);
                for s in steps {
                    set = self.eval_step(set, s)?;
                }
                Ok(Value::Nodes(self.normalize(set)))
            }
            Ast::Filter { base, preds, steps } => {
                let v = self.eval(base, ctx)?;
                let Value::Nodes(items) = v else {
                    if preds.is_empty() && steps.is_empty() {
                        return self.eval(base, ctx);
                    }
                    return err("only a node-set can take a predicate or a further step");
                };
                let mut items = items;
                for pred in preds {
                    items = self.filter(items, pred, false)?;
                }
                let mut set = Set::new(items, true);
                for s in steps {
                    set = self.eval_step(set, s)?;
                }
                Ok(Value::Nodes(self.normalize(set)))
            }
            Ast::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, ctx),
            Ast::Call { name, args } => self.call(name, args, ctx),
        }
    }

    fn binary(&self, op: Op, lhs: &Ast, rhs: &Ast, ctx: &Ctx) -> R<Value> {
        // `or` and `and` short-circuit, as XPath allows.
        if op == Op::Or || op == Op::And {
            let a = self.eval(lhs, ctx).map(|v| self.to_bool(&v))?;
            if op == Op::Or && a {
                return Ok(Value::Bool(true));
            }
            if op == Op::And && !a {
                return Ok(Value::Bool(false));
            }
            let b = self.eval(rhs, ctx).map(|v| self.to_bool(&v))?;
            return Ok(Value::Bool(b));
        }
        let a = self.eval(lhs, ctx)?;
        let b = self.eval(rhs, ctx)?;
        Ok(match op {
            Op::Union => {
                let (Value::Nodes(x), Value::Nodes(y)) = (&a, &b) else { return err("`|` joins two node-sets") };
                let mut items = x.clone();
                items.extend(y.iter().copied());
                Value::Nodes(self.normalize(Set::new(items, false)))
            }
            Op::Eq | Op::Ne => Value::Bool(self.compare_eq(&a, &b, op == Op::Ne)),
            Op::Lt | Op::Le | Op::Gt | Op::Ge => Value::Bool(self.compare_rel(&a, &b, op)),
            Op::Add => Value::Number(self.to_number(&a) + self.to_number(&b)),
            Op::Sub => Value::Number(self.to_number(&a) - self.to_number(&b)),
            Op::Mul => Value::Number(self.to_number(&a) * self.to_number(&b)),
            Op::Div => Value::Number(self.to_number(&a) / self.to_number(&b)),
            Op::Mod => Value::Number(self.to_number(&a) % self.to_number(&b)),
            Op::Or | Op::And => unreachable!(),
        })
    }

    // ---- conversions ----------------------------------------------------------------------

    fn string_of(&self, item: Item) -> String {
        match item {
            Item::Attr { index, .. } => self.doc.attr_value_at(index).to_string(),
            Item::Node(n) => self.doc.string_value(n).into_owned(),
        }
    }

    fn to_bool(&self, v: &Value) -> bool {
        match v {
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            Value::Nodes(items) => !items.is_empty(),
        }
    }

    fn to_string(&self, v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
            Value::Number(n) => number_to_string(*n),
            Value::Nodes(items) => items.first().map(|i| self.string_of(*i)).unwrap_or_default(),
        }
    }

    fn to_number(&self, v: &Value) -> f64 {
        match v {
            Value::Number(n) => *n,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::String(s) => string_to_number(s),
            Value::Nodes(_) => string_to_number(&self.to_string(v)),
        }
    }

    fn compare_eq(&self, a: &Value, b: &Value, negate: bool) -> bool {
        let r = match (a, b) {
            (Value::Nodes(x), Value::Nodes(y)) => {
                let ys: Vec<String> = y.iter().map(|i| self.string_of(*i)).collect();
                x.iter().any(|i| {
                    let s = self.string_of(*i);
                    ys.iter().any(|t| (*t == s) != negate)
                })
            }
            (Value::Nodes(x), other) | (other, Value::Nodes(x)) => match other {
                Value::Bool(_) => (self.to_bool(&Value::Nodes(x.clone())) == self.to_bool(other)) != negate,
                Value::Number(n) => x.iter().any(|i| (string_to_number(&self.string_of(*i)) == *n) != negate),
                _ => {
                    let s = self.to_string(other);
                    x.iter().any(|i| (self.string_of(*i) == s) != negate)
                }
            },
            (Value::Bool(_), _) | (_, Value::Bool(_)) => (self.to_bool(a) == self.to_bool(b)) != negate,
            (Value::Number(_), _) | (_, Value::Number(_)) => {
                let (x, y) = (self.to_number(a), self.to_number(b));
                (x == y) != negate
            }
            _ => (self.to_string(a) == self.to_string(b)) != negate,
        };
        // The node-set arms already folded `negate` in; the rest compared and then negated.
        r
    }

    fn compare_rel(&self, a: &Value, b: &Value, op: Op) -> bool {
        let cmp = |x: f64, y: f64| match op {
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            Op::Ge => x >= y,
            _ => false,
        };
        match (a, b) {
            (Value::Nodes(x), Value::Nodes(y)) => {
                let ys: Vec<f64> = y.iter().map(|i| string_to_number(&self.string_of(*i))).collect();
                x.iter().any(|i| {
                    let n = string_to_number(&self.string_of(*i));
                    ys.iter().any(|m| cmp(n, *m))
                })
            }
            (Value::Nodes(x), other) => {
                let n = self.to_number(other);
                x.iter().any(|i| cmp(string_to_number(&self.string_of(*i)), n))
            }
            (other, Value::Nodes(y)) => {
                let n = self.to_number(other);
                y.iter().any(|i| cmp(n, string_to_number(&self.string_of(*i))))
            }
            _ => cmp(self.to_number(a), self.to_number(b)),
        }
    }

    // ---- functions --------------------------------------------------------------------------

    fn arg_string(&self, args: &[Ast], i: usize, ctx: &Ctx) -> R<String> {
        match args.get(i) {
            Some(a) => Ok(self.to_string(&self.eval(a, ctx)?)),
            None => Ok(self.string_of(ctx.item)),
        }
    }

    fn call(&self, name: &str, args: &[Ast], ctx: &Ctx) -> R<Value> {
        let nodes = |i: usize| -> R<Vec<Item>> {
            match self.eval(&args[i], ctx)? {
                Value::Nodes(n) => Ok(n),
                _ => err(format!("`{name}()` needs a node-set")),
            }
        };
        Ok(match name {
            "last" => Value::Number(ctx.size as f64),
            "position" => Value::Number(ctx.position as f64),
            "count" => Value::Number(nodes(0)?.len() as f64),
            "id" => Value::Nodes(Vec::new()),
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "lang" => Value::Bool(false),
            "namespace-uri" => Value::String(String::new()),
            "not" => Value::Bool(!self.to_bool(&self.eval(&args[0], ctx)?)),
            "boolean" => Value::Bool(self.to_bool(&self.eval(&args[0], ctx)?)),
            "string" => Value::String(match args.first() {
                Some(a) => self.to_string(&self.eval(a, ctx)?),
                None => self.string_of(ctx.item),
            }),
            "number" => Value::Number(match args.first() {
                Some(a) => self.to_number(&self.eval(a, ctx)?),
                None => string_to_number(&self.string_of(ctx.item)),
            }),
            "name" | "local-name" => {
                let item = match args.first() {
                    None => Some(ctx.item),
                    Some(a) => match self.eval(a, ctx)? {
                        Value::Nodes(n) => n.first().copied(),
                        _ => return err(format!("`{name}()` needs a node-set")),
                    },
                };
                Value::String(match item {
                    None => String::new(),
                    Some(Item::Attr { index, .. }) => self.doc.syms.get(self.doc.attr_name_at(index)).to_string(),
                    Some(Item::Node(n)) if self.doc.is_element(n) => self.doc.name(n).to_string(),
                    Some(_) => String::new(),
                })
            }
            "concat" => {
                let mut s = String::new();
                for a in args {
                    let v = self.eval(a, ctx)?;
                    let _ = write!(s, "{}", self.to_string(&v));
                }
                Value::String(s)
            }
            "starts-with" => Value::Bool(self.arg_string(args, 0, ctx)?.starts_with(&self.arg_string(args, 1, ctx)?)),
            "contains" => Value::Bool(self.arg_string(args, 0, ctx)?.contains(&self.arg_string(args, 1, ctx)?)),
            "substring-before" => {
                let (s, t) = (self.arg_string(args, 0, ctx)?, self.arg_string(args, 1, ctx)?);
                Value::String(s.find(&t).map(|i| s[..i].to_string()).unwrap_or_default())
            }
            "substring-after" => {
                let (s, t) = (self.arg_string(args, 0, ctx)?, self.arg_string(args, 1, ctx)?);
                Value::String(s.find(&t).map(|i| s[i + t.len()..].to_string()).unwrap_or_default())
            }
            "substring" => {
                let s: Vec<char> = self.arg_string(args, 0, ctx)?.chars().collect();
                let from = round_half_up(self.to_number(&self.eval(&args[1], ctx)?));
                // XPath counts from 1 and rounds both arguments before slicing.
                let (lo, hi) = match args.get(2) {
                    None => (from, f64::INFINITY),
                    Some(a) => {
                        let len = round_half_up(self.to_number(&self.eval(a, ctx)?));
                        (from, from + len)
                    }
                };
                let mut out = String::new();
                for (i, c) in s.iter().enumerate() {
                    let pos = (i + 1) as f64;
                    if pos >= lo && pos < hi {
                        out.push(*c);
                    }
                }
                Value::String(out)
            }
            "string-length" => Value::Number(self.arg_string(args, 0, ctx)?.chars().count() as f64),
            "normalize-space" => Value::String(self.arg_string(args, 0, ctx)?.split_whitespace().collect::<Vec<_>>().join(" ")),
            "translate" => {
                let (s, from, to) = (self.arg_string(args, 0, ctx)?, self.arg_string(args, 1, ctx)?, self.arg_string(args, 2, ctx)?);
                let from: Vec<char> = from.chars().collect();
                let to: Vec<char> = to.chars().collect();
                let mut out = String::new();
                for c in s.chars() {
                    match from.iter().position(|f| *f == c) {
                        None => out.push(c),
                        Some(i) => {
                            if let Some(r) = to.get(i) {
                                out.push(*r);
                            }
                        }
                    }
                }
                Value::String(out)
            }
            "sum" => Value::Number(nodes(0)?.iter().map(|i| string_to_number(&self.string_of(*i))).sum()),
            "floor" => Value::Number(self.to_number(&self.eval(&args[0], ctx)?).floor()),
            "ceiling" => Value::Number(self.to_number(&self.eval(&args[0], ctx)?).ceil()),
            "round" => Value::Number(round_half_up(self.to_number(&self.eval(&args[0], ctx)?))),
            other => return err(format!("there is no function called `{other}()`")),
        })
    }
}

/// Does this predicate depend on where the node sits in the set?
fn is_positional(ast: &Ast) -> bool {
    match ast {
        Ast::Number(_) => true,
        Ast::Call { name, args } => name == "position" || name == "last" || args.iter().any(is_positional),
        Ast::Binary { lhs, rhs, .. } => is_positional(lhs) || is_positional(rhs),
        Ast::Neg(e) => is_positional(e),
        Ast::Filter { base, preds, .. } => is_positional(base) || preds.iter().any(is_positional),
        _ => false,
    }
}

enum Key {
    DefName(String),
    NameAttr(String),
}

/// `defName="X"`, `@Name="X"`, and `or`-chains of them — the keys the arena indexes.
fn lookup_keys(pred: &Ast) -> Option<Vec<Key>> {
    match pred {
        Ast::Binary { op: Op::Or, lhs, rhs } => {
            let mut a = lookup_keys(lhs)?;
            a.extend(lookup_keys(rhs)?);
            Some(a)
        }
        Ast::Binary { op: Op::Eq, lhs, rhs } => {
            let (path, lit) = match (lhs.as_ref(), rhs.as_ref()) {
                (p, Ast::Literal(s)) => (p, s),
                (Ast::Literal(s), p) => (p, s),
                _ => return None,
            };
            let Ast::Path { absolute: false, steps } = path else { return None };
            // `defName`, `./defName`, `child::defName`, `defName/text()`
            let mut steps: Vec<&Step> = steps.iter().collect();
            if steps.first().map(|s| s.axis == Axis::Zelf && s.preds.is_empty()).unwrap_or(false) {
                steps.remove(0);
            }
            if steps.last().map(|s| s.test == NodeTest::Text && s.axis == Axis::Child && s.preds.is_empty()).unwrap_or(false) {
                steps.pop();
            }
            let [only] = steps[..] else { return None };
            if !only.preds.is_empty() {
                return None;
            }
            match (&only.axis, &only.test) {
                (Axis::Child, NodeTest::Name(n)) if n == "defName" => Some(vec![Key::DefName(lit.clone())]),
                (Axis::Attribute, NodeTest::Name(n)) if n == "Name" => Some(vec![Key::NameAttr(lit.clone())]),
                _ => None,
            }
        }
        _ => None,
    }
}

/// XPath's `round()`: halves go up, and −0 stays −0.
fn round_half_up(n: f64) -> f64 {
    if n.is_nan() || n.is_infinite() {
        return n;
    }
    (n + 0.5).floor()
}

/// XPath's `number()` over a string: optional sign, digits, one dot, nothing else.
fn string_to_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return f64::NAN;
    }
    let mut chars = t.chars();
    let rest = match chars.next() {
        Some('-') | Some('+') => chars.as_str(),
        _ => t,
    };
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit() || c == '.') || rest.matches('.').count() > 1 || rest == "." {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// XPath's `string()` over a number: integers without a point, no exponent, no negative zero.
fn number_to_string(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    if n == n.trunc() && n.abs() < 1e21 {
        return format!("{}", n as i64);
    }
    let mut s = format!("{n}");
    if s.contains('e') {
        // XPath has no exponent form; write it out.
        s = format!("{n:.*}", 20);
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

// ---------------------------------------------------------------------------------------------
// The public surface
// ---------------------------------------------------------------------------------------------

/// Evaluate an expression against `ctx` (normally `DOCUMENT`, the node .NET starts from).
pub fn evaluate(doc: &Doc, ctx: NodeId, expr: &Expr) -> R<Value> {
    let e = Eval { doc };
    e.eval(&expr.ast, &Ctx { item: Item::Node(ctx), position: 1, size: 1 })
}

/// The node-set an expression selects, in document order and without duplicates.
pub fn select(doc: &Doc, ctx: NodeId, expr: &Expr) -> R<Vec<Item>> {
    match evaluate(doc, ctx, expr)? {
        Value::Nodes(items) => Ok(items),
        _ => err(format!("`{}` does not select nodes", expr.text())),
    }
}

pub fn select_first(doc: &Doc, ctx: NodeId, expr: &Expr) -> R<Option<Item>> {
    Ok(select(doc, ctx, expr)?.into_iter().next())
}

/// Compile and select in one call; for tests and one-off queries.
pub fn select_str(doc: &Doc, ctx: NodeId, xpath: &str) -> R<Vec<Item>> {
    select(doc, ctx, &compile(xpath)?)
}

/// The tag of an item, for messages: the element's name, `@attr`, or `text()`.
pub fn item_name(doc: &Doc, item: Item) -> String {
    match item {
        Item::Attr { index, .. } => format!("@{}", doc.syms.get(doc.attr_name_at(index))),
        Item::Node(n) => match doc.kind(n) {
            Kind::Element => doc.name(n).to_string(),
            Kind::Text => "text()".into(),
            Kind::Document => "/".into(),
        },
    }
}

/// The symbol for a tag, interning it if the document has not seen it. Patch operations need
/// this to add elements the document does not contain yet.
pub fn intern(doc: &mut Doc, name: &str) -> Sym {
    doc.syms.intern(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defs::tree::import_children;

    const DOC: &str = r#"<Defs>
  <ThingDef Name="BuildingBase" Abstract="True"><statBases><MaxHitPoints>100</MaxHitPoints><Flammability>1.0</Flammability></statBases><comps><li Class="CompProperties_Forbiddable"/></comps></ThingDef>
  <ThingDef ParentName="BuildingBase"><defName>Wall</defName><label>wall</label><thingClass>Building</thingClass><statBases><MaxHitPoints>300</MaxHitPoints></statBases><costList><Steel>5</Steel><WoodLog>3</WoodLog></costList><comps><li Class="CompProperties_A"><foo>1</foo></li><li Class="CompProperties_B"/></comps></ThingDef>
  <ThingDef ParentName="BuildingBase"><defName>Door</defName><label>door</label><statBases><MaxHitPoints>160</MaxHitPoints></statBases><comps><li Class="CompProperties_B"/></comps></ThingDef>
  <PawnKindDef><defName>Colonist</defName><label>colonist</label><race>Human</race></PawnKindDef>
  <RecipeDef><defName>Make_Wall</defName><products><Wall>1</Wall></products><ingredients><li><count>2</count></li><li><count>7</count></li></ingredients></RecipeDef>
</Defs>"#;

    fn doc() -> Doc {
        let mut d = Doc::new();
        let root = d.new_element_named("Defs", 0);
        d.append_child(DOCUMENT, root);
        import_children(&mut d, DOC, root, 1).unwrap();
        d
    }

    fn sel(d: &Doc, x: &str) -> Vec<Item> {
        select_str(d, DOCUMENT, x).unwrap_or_else(|e| panic!("{x}: {}", e.0))
    }
    fn n(d: &Doc, x: &str) -> usize {
        sel(d, x).len()
    }
    /// The string-values of what an expression selects.
    fn strs(d: &Doc, x: &str) -> Vec<String> {
        let e = Eval { doc: d };
        sel(d, x).into_iter().map(|i| e.string_of(i)).collect()
    }
    fn val(d: &Doc, x: &str) -> Value {
        evaluate(d, DOCUMENT, &compile(x).unwrap()).unwrap()
    }

    #[test]
    fn the_shapes_patches_actually_use() {
        let d = doc();
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]"#), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName='Door']/statBases/MaxHitPoints"#), ["160"]);
        assert_eq!(strs(&d, r#"/Defs/ThingDef[defName = "Wall"]/costList/Steel"#), ["5"]);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall" or defName="Door"]/comps"#), 2);
        assert_eq!(n(&d, r#"Defs/ThingDef[@Name="BuildingBase"]"#), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[@Name="BuildingBase"]/@Abstract"#), ["True"]);
        assert_eq!(n(&d, "Defs/ThingDef[not(@Abstract)]"), 2);
        assert_eq!(n(&d, r#"Defs/ThingDef[@Abstract="True"]"#), 1);
        assert_eq!(n(&d, "Defs/*[defName]"), 4);
        assert_eq!(n(&d, "Defs/ThingDef[not(defName)]"), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Nope"]"#), 0);
        // names are case-sensitive, as they are in .NET
        assert_eq!(n(&d, r#"Defs/thingdef[defName="Wall"]"#), 0);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="wall"]"#), 0);
    }

    #[test]
    fn axes_and_positions() {
        let d = doc();
        assert_eq!(n(&d, r#"//comps/li[@Class="CompProperties_B"]"#), 2);
        // document order: Wall's before Door's
        let both = strs(&d, r#"//li[@Class="CompProperties_B"]/../../defName"#);
        assert_eq!(both, ["Wall", "Door"]);
        assert_eq!(n(&d, r#"//li[@Class="CompProperties_B"]/.."#), 2);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[2]/@Class"#), ["CompProperties_B"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[last()]/@Class"#), ["CompProperties_B"]);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[position()>1]"#), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[1]/@Class"#), ["CompProperties_A"]);
        // a reverse axis counts away from the context node
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[@Class="CompProperties_B"]/preceding-sibling::li[1]/@Class"#), ["CompProperties_A"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[1]/following-sibling::li[1]/@Class"#), ["CompProperties_B"]);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/comps/li[1]/following-sibling::*"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/../PawnKindDef"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/ancestor::Defs"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/descendant::li"#), 2);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]//foo"#), 1);
        assert_eq!(n(&d, ".//foo"), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/self::ThingDef"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/self::Foo"#), 0);
        assert_eq!(strs(&d, "(//ThingDef)[2]/defName"), ["Wall"]);
        assert_eq!(strs(&d, "Defs/ThingDef[last()]/defName"), ["Door"]);
        assert_eq!(n(&d, "Defs/ThingDef[1]/defName"), 0, "the first ThingDef is the abstract base and has no defName");
        // a union comes back in document order whichever way round it is written
        let a = r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints | Defs/ThingDef[defName="Door"]/statBases/MaxHitPoints"#;
        let b = r#"Defs/ThingDef[defName="Door"]/statBases/MaxHitPoints | Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#;
        assert_eq!(strs(&d, a), ["300", "160"]);
        assert_eq!(strs(&d, a), strs(&d, b));
    }

    #[test]
    fn comparisons_and_functions() {
        let d = doc();
        assert_eq!(strs(&d, "Defs/ThingDef[statBases/MaxHitPoints>200]/defName"), ["Wall"]);
        assert_eq!(n(&d, "Defs/ThingDef[statBases/MaxHitPoints > 100]"), 2);
        assert_eq!(strs(&d, "Defs/ThingDef[costList/Steel=5]/defName"), ["Wall"]);
        assert_eq!(val(&d, "count(Defs/ThingDef)"), Value::Number(3.0));
        assert_eq!(val(&d, "count(//li)"), Value::Number(6.0));
        assert_eq!(val(&d, "sum(Defs/RecipeDef/ingredients/li/count)"), Value::Number(9.0));
        assert_eq!(n(&d, "Defs/RecipeDef/ingredients/li[count>5]"), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[contains(defName,"oo")]/defName"#), ["Door"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[starts-with(defName,"Wa")]/defName"#), ["Wall"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[label="wall"]/defName"#), ["Wall"]);
        assert_eq!(n(&d, r#"Defs/*[label="colonist"]"#), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall"]/label/text()"#), ["wall"]);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/label[text()="wall"]"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[label/text()="wall"]"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"]/comps[li/@Class="CompProperties_A"]"#), 1);
        assert_eq!(n(&d, r#"Defs/ThingDef[comps/li[@Class="CompProperties_B"] and defName="Door"]"#), 1);
        assert_eq!(strs(&d, r#"Defs/ThingDef[normalize-space(label)="wall"]/defName"#), ["Wall"]);
        assert_eq!(n(&d, "Defs/ThingDef[string-length(defName)=4]"), 2);
        assert_eq!(strs(&d, r#"Defs/ThingDef[translate(defName,"W","w")="wall"]/defName"#), ["Wall"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[substring(defName,1,2)="Do"]/defName"#), ["Door"]);
        assert_eq!(n(&d, r#"Defs/ThingDef[concat(defName,"!")="Wall!"]"#), 1);
        assert_eq!(strs(&d, "Defs/ThingDef[number(statBases/MaxHitPoints) mod 100 = 60]/defName"), ["Door"]);
        assert_eq!(strs(&d, "Defs/ThingDef[floor(statBases/MaxHitPoints div 100)=3]/defName"), ["Wall"]);
        assert_eq!(n(&d, "Defs/ThingDef[boolean(comps)]"), 3);
        assert_eq!(n(&d, "Defs/ThingDef[string(defName)]"), 2);
        assert_eq!(n(&d, r#"Defs/ThingDef[name()="ThingDef"]"#), 3);
        assert_eq!(n(&d, r#"Defs/*[local-name()="RecipeDef"]"#), 1);
    }

    #[test]
    fn the_conversion_rules() {
        let d = doc();
        assert_eq!(val(&d, "1 + 2 * 3"), Value::Number(7.0));
        assert_eq!(val(&d, r#""a" = "a""#), Value::Bool(true));
        assert_eq!(val(&d, "string(1.50)"), Value::String("1.5".into()));
        assert_eq!(val(&d, "string(100)"), Value::String("100".into()));
        assert_eq!(val(&d, "string(-3)"), Value::String("-3".into()));
        assert_eq!(val(&d, "string(1 div 0)"), Value::String("Infinity".into()));
        assert_eq!(val(&d, "string(-1 div 0)"), Value::String("-Infinity".into()));
        assert_eq!(val(&d, r#"string(number("x"))"#), Value::String("NaN".into()));
        assert_eq!(val(&d, r#"number("x") = number("x")"#), Value::Bool(false));
        assert_eq!(val(&d, r#"not(number("x"))"#), Value::Bool(true));
        assert_eq!(val(&d, r#"substring("12345", 1.5, 2.6)"#), Value::String("234".into()));
        assert_eq!(val(&d, r#"substring("12345", 0, 3)"#), Value::String("12".into()));
        assert_eq!(val(&d, r#"substring("12345", 2)"#), Value::String("2345".into()));
        assert_eq!(val(&d, r#"normalize-space("  a   b ")"#), Value::String("a b".into()));
        assert_eq!(val(&d, "round(2.5)"), Value::Number(3.0));
        assert_eq!(val(&d, "round(-2.5)"), Value::Number(-2.0));
        assert_eq!(val(&d, "floor(-1.5)"), Value::Number(-2.0));
        assert_eq!(val(&d, "ceiling(1.1)"), Value::Number(2.0));
        assert_eq!(val(&d, r#"string-length("héllo")"#), Value::Number(5.0));
        assert_eq!(val(&d, r#"translate("bar","abc","ABC")"#), Value::String("BAr".into()));
        assert_eq!(val(&d, r#"substring-before("a/b","/")"#), Value::String("a".into()));
        assert_eq!(val(&d, r#"substring-after("a/b","/")"#), Value::String("b".into()));
        assert_eq!(val(&d, r#"contains("abc","")"#), Value::Bool(true));
        assert_eq!(val(&d, "10 mod 3"), Value::Number(1.0));
        assert_eq!(val(&d, "-7 mod 2"), Value::Number(-1.0));
        assert_eq!(val(&d, "5 div 2"), Value::Number(2.5));
        assert_eq!(val(&d, "-(2 + 3)"), Value::Number(-5.0));
    }

    #[test]
    fn errors_read_like_dot_nets() {
        assert!(compile(r#"Defs/ThingDef[ends-with(defName,'x')]"#).is_err());
        assert!(compile(r#"Defs/ThingDef[defName="Wall""#).is_err());
        assert!(compile("$var").is_err());
        assert!(compile("").is_err());
        assert!(compile("   ").is_err());
        assert!(compile("a:b").is_err());
        assert!(compile(r#"Defs/ThingDef[defName="Wall"]/@"#).is_err());
        assert!(compile("count()").is_err(), "count() takes one argument");
        assert!(compile("Defs/ThingDef[").is_err());
        let d = doc();
        assert!(select(&d, DOCUMENT, &compile("1+1").unwrap()).is_err(), "a number is not a node-set");
        assert!(compile("Defs/ThingDef").is_ok());
    }

    #[test]
    fn the_fast_path_agrees_with_the_walk() {
        let d = doc();
        let want = sel(&d, r#"Defs/ThingDef[defName="Wall"]"#);
        for x in [r#"Defs/ThingDef[./defName="Wall"]"#, r#"Defs/ThingDef[defName/text()="Wall"]"#, r#"Defs/ThingDef["Wall"=defName]"#, r#"Defs/ThingDef[child::defName="Wall"]"#] {
            assert_eq!(sel(&d, x), want, "{x}");
        }
        assert_eq!(n(&d, r#"Defs/ThingDef[defName="Wall"][2]"#), 0);
        // a positional predicate after the lookup counts over the matches, not the whole list
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall" or defName="Door"][2]/defName"#), ["Door"]);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Wall" or defName="Door"][1]/defName"#), ["Wall"]);
        // a tag the document has never seen selects nothing rather than failing
        assert_eq!(n(&d, r#"Defs/NoSuchDef[defName="Wall"]"#), 0);
    }

    #[test]
    fn duplicates_and_mutation() {
        let mut d = Doc::new();
        let root = d.new_element_named("Defs", 0);
        d.append_child(DOCUMENT, root);
        import_children(&mut d, "<Defs><ThingDef><defName>Dup</defName><label>one</label></ThingDef><ThingDef><defName>Dup</defName><label>two</label></ThingDef></Defs>", root, 1).unwrap();
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Dup"]/label"#), ["one", "two"]);
        let first = d.element_children(root).next().unwrap();
        d.remove(first);
        assert_eq!(strs(&d, r#"Defs/ThingDef[defName="Dup"]/label"#), ["two"]);
        // the walk sees the same thing as the index
        assert_eq!(strs(&d, r#"Defs/ThingDef[./defName="Dup"]/label"#), ["two"]);
    }

    #[test]
    fn string_values_are_not_trimmed() {
        let mut d = Doc::new();
        let root = d.new_element_named("Defs", 0);
        d.append_child(DOCUMENT, root);
        import_children(&mut d, "<Defs><ThingDef><defName>Wall</defName><label>  wall </label></ThingDef></Defs>", root, 1).unwrap();
        assert_eq!(n(&d, r#"Defs/ThingDef[label="wall"]"#), 0);
        assert_eq!(n(&d, r#"Defs/ThingDef[normalize-space(label)="wall"]"#), 1);
    }
}
