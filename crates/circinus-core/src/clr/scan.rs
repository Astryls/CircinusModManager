//! What an assembly patches, read from its attributes and its IL.
//!
//! The behaviour here is the sidecar's (`tools/harmony-scan`), kept the same on purpose: its
//! answers for real mods are the oracle the tests compare against. Harmony builds a patch
//! target by layering attributes — the class supplies defaults, the method refines them field
//! by field — and finds its ids and manual patches in code, which is read as a stream of
//! instructions rather than simulated. One thing is added beyond the sidecar: calls to
//! `AccessTools` that name a method are recorded even when no `Patch(...)` follows, because
//! the RimWorld frameworks that wrap Harmony in their own helpers are invisible otherwise.

use super::blob::{self, Attribute, Value};
use super::il;
use super::metadata::{Metadata, Table};
use super::pe::Pe;
use crate::harmony::{AssemblyPatches, ManualPatch, PatchTarget};
use std::collections::{HashMap, HashSet, VecDeque};

/// The pieces a `[HarmonyPatch]` attribute can carry. Every field is optional precisely so
/// "not stated" and "stated as empty" stay distinguishable when attributes are layered.
#[derive(Debug, Clone, Default)]
struct PatchSpec {
    declaring_type: Option<String>,
    method_name: Option<String>,
    method_type: Option<i32>,
    argument_types: Option<Vec<String>>,
}

impl PatchSpec {
    /// Layer `over` on top of this one; anything it states wins.
    fn apply(&mut self, over: PatchSpec) {
        if over.declaring_type.is_some() {
            self.declaring_type = over.declaring_type;
        }
        if over.method_name.is_some() {
            self.method_name = over.method_name;
        }
        if over.method_type.is_some() {
            self.method_type = over.method_type;
        }
        if over.argument_types.is_some() {
            self.argument_types = over.argument_types;
        }
    }
}

/// Priority and ordering hints, which layer the same way a `PatchSpec` does.
#[derive(Debug, Clone, Default)]
struct PatchOrdering {
    priority: Option<i32>,
    before: Option<Vec<String>>,
    after: Option<Vec<String>>,
}

impl PatchOrdering {
    fn apply(&mut self, over: PatchOrdering) {
        if over.priority.is_some() {
            self.priority = over.priority;
        }
        if over.before.is_some() {
            self.before = over.before;
        }
        if over.after.is_some() {
            self.after = over.after;
        }
    }
}

/// Patch-method names Harmony recognises without an attribute, matched case-insensitively.
const KIND_BY_NAME: [(&str, &str); 4] = [("Prefix", "prefix"), ("Postfix", "postfix"), ("Transpiler", "transpiler"), ("Finalizer", "finalizer")];

fn kind_by_name(method: &str) -> Option<&'static str> {
    KIND_BY_NAME.iter().find(|(n, _)| n.eq_ignore_ascii_case(method)).map(|(_, k)| *k)
}

/// The trailing word of names like `Pawn_Tick_Prefix`. Harmony itself does not do this, but a
/// method that brought its own `[HarmonyPatch]` and ends in "Prefix" is a prefix in every mod
/// ever written, and saying so is more useful than dropping the row.
fn kind_by_suffix(method: &str) -> Option<&'static str> {
    KIND_BY_NAME.iter().find(|(n, _)| method.len() > n.len() && method.get(method.len() - n.len()..).is_some_and(|tail| tail.eq_ignore_ascii_case(n))).map(|(_, k)| *k)
}

/// Harmony's `MethodType` enum, from the integer in the blob.
fn method_type_name(value: Option<i32>) -> &'static str {
    match value {
        Some(1) => "getter",
        Some(2) => "setter",
        Some(3) => "constructor",
        Some(4) => "staticConstructor",
        Some(5) => "enumerator",
        Some(6) => "async",
        _ => "normal",
    }
}

/// Drop the assembly qualification from a serialized type name, leaving `RimWorld.Pawn`.
/// Commas inside `[…]` belong to generic arguments and stay.
fn clean(name: &str) -> String {
    let mut depth = 0i32;
    for (i, c) in name.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => return name[..i].trim().to_string(),
            _ => {}
        }
    }
    name.trim().to_string()
}

// Harmony's own surface, matched by name so that ILMerged and namespace-rewritten copies (which
// RimWorld mods really do ship) are still recognised.
fn is_harmony_runner(ty: &str) -> bool {
    ty == "HarmonyLib.Harmony" || ty == "Harmony.HarmonyInstance" || ty.ends_with(".Harmony") || ty.ends_with(".HarmonyInstance")
}

fn is_access_tools(ty: &str) -> bool {
    ty == "AccessTools" || ty.ends_with(".AccessTools")
}

/// The arguments of the next reflection call, as the `AccessTools` rows see them: the
/// `typeof` before the name, the name, and whether the `ldtoken`s now being pushed belong to a
/// `new Type[] { … }` argument rather than to the declaring type.
#[derive(Debug, Default)]
struct Reach {
    ty: Option<String>,
    name: Option<String>,
    in_array: bool,
}

pub(super) struct Scanner<'a> {
    pe: &'a Pe<'a>,
    md: &'a Metadata<'a>,
    out: &'a mut AssemblyPatches,
    /// CustomAttribute rows by TypeDef row and by MethodDef row, bucketed once up front so the
    /// per-type work does not rescan the whole table.
    type_attrs: HashMap<u32, Vec<u32>>,
    method_attrs: HashMap<u32, Vec<u32>>,
    seen_manual: HashSet<(String, String, String)>,
    seen_reach: HashSet<(String, String, String, String, &'static str)>,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(pe: &'a Pe<'a>, md: &'a Metadata<'a>, out: &'a mut AssemblyPatches) -> Self {
        let mut type_attrs: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut method_attrs: HashMap<u32, Vec<u32>> = HashMap::new();
        for row in 1..=md.rows(Table::CustomAttribute) {
            match md.custom_attribute_parent(row) {
                Some((Table::TypeDef, parent)) => type_attrs.entry(parent).or_default().push(row),
                Some((Table::MethodDef, parent)) => method_attrs.entry(parent).or_default().push(row),
                _ => {}
            }
        }
        Scanner { pe, md, out, type_attrs, method_attrs, seen_manual: HashSet::new(), seen_reach: HashSet::new() }
    }

    pub(super) fn run(&mut self) {
        self.out.name = self.md.assembly_name().map(str::to_string);
        self.out.mvid = self.md.module_mvid();
        let harmony = self.uses_harmony();
        for ty in 1..=self.md.rows(Table::TypeDef) {
            self.read_type(ty, harmony);
        }
    }

    /// Whether the assembly has anything to do with Harmony: a reference to 0Harmony, a type
    /// reference into its namespaces, or — for the mods that ILMerge it — its types outright.
    /// Assemblies that fail all three are the common case and skip the expensive work.
    fn uses_harmony(&self) -> bool {
        let md = self.md;
        let harmony_namespace = |ns: &str| ns == "HarmonyLib" || ns == "Harmony";
        md.assembly_ref_names().any(|n| n == "0Harmony" || n == "HarmonyLib" || n == "Harmony")
            || (1..=md.rows(Table::TypeRef)).any(|r| md.type_ref_namespace(r).is_some_and(harmony_namespace))
            || (1..=md.rows(Table::TypeDef)).any(|r| md.type_def_namespace(r).is_some_and(harmony_namespace))
    }

    // -- attributes ---------------------------------------------------------------------------

    /// Name of the attribute type a CustomAttribute row constructs, always in its `…Attribute`
    /// form: HarmonyLib declares `HarmonyPatch`, Verse declares `StaticConstructorOnStartup`,
    /// and C# lets either spelling appear at a use site, so callers should not have to care.
    fn attribute_name(&self, row: u32) -> String {
        let md = self.md;
        let name = match md.custom_attribute_ctor(row) {
            Some((Table::MethodDef, ctor)) => md.method_owner(ctor).and_then(|owner| md.type_def_short_name(owner)),
            Some((Table::MemberRef, ctor)) => match md.member_ref_parent(ctor) {
                Some((Table::TypeRef, parent)) => md.type_ref_short_name(parent),
                Some((Table::TypeDef, parent)) => md.type_def_short_name(parent),
                _ => None,
            },
            _ => None,
        }
        .unwrap_or("");
        if name.is_empty() || name.ends_with("Attribute") {
            name.to_string()
        } else {
            format!("{name}Attribute")
        }
    }

    /// The attribute's arguments, typed by its constructor's signature.
    fn decode(&self, row: u32) -> Option<Attribute> {
        let md = self.md;
        let signature = match md.custom_attribute_ctor(row)? {
            (Table::MethodDef, ctor) => md.method_signature(ctor)?,
            (Table::MemberRef, ctor) => md.member_ref_signature(ctor)?,
            _ => return None,
        };
        let params = blob::method_parameters(md, signature)?;
        blob::decode_attribute(md.custom_attribute_value(row).unwrap_or(&[]), &params)
    }

    /// Fold one `[HarmonyPatch(…)]` into a `PatchSpec`. Harmony declares a dozen constructor
    /// overloads and they cannot be told apart by arity, so the arguments are read by their
    /// decoded types instead: a `System.Type` is the declaring type, a `Type[]` is the
    /// argument-type list, an integer is a `MethodType`, and strings are positional — one is a
    /// method name, two are (typeName, methodName).
    fn read_patch(&self, row: u32) -> PatchSpec {
        let mut spec = PatchSpec::default();
        let Some(attr) = self.decode(row) else { return spec };
        let mut strings = Vec::new();
        for (ty, value) in &attr.fixed {
            match value {
                Value::Type(Some(s)) if ty.is_system_type() => {
                    spec.declaring_type.get_or_insert_with(|| clean(s));
                }
                Value::Str(Some(s)) => strings.push(s.clone()),
                Value::Int32(i) => {
                    spec.method_type.get_or_insert(*i);
                }
                Value::Array(Some(items)) => {
                    if let Some(types) = type_array(items) {
                        spec.argument_types.get_or_insert(types);
                    }
                }
                _ => {}
            }
        }
        if strings.len() >= 2 {
            // (string typeName, string methodName[, MethodType]) — the reflection-only form
            // mods use when the target type is not public.
            spec.declaring_type.get_or_insert_with(|| clean(&strings[0]));
            spec.method_name.get_or_insert_with(|| strings[1].clone());
        } else if let Some(only) = strings.first() {
            spec.method_name.get_or_insert_with(|| only.clone());
        }
        for (name, value) in &attr.named {
            match (name.as_str(), value) {
                ("declaringType" | "DeclaringType", Value::Str(Some(s)) | Value::Type(Some(s))) => spec.declaring_type = Some(clean(s)),
                ("methodName" | "MethodName", Value::Str(Some(s)) | Value::Type(Some(s))) => spec.method_name = Some(s.clone()),
                ("methodType" | "MethodType", Value::Int32(i)) => spec.method_type = Some(*i),
                ("argumentTypes" | "ArgumentTypes", Value::Array(Some(items))) => {
                    if let Some(types) = type_array(items) {
                        spec.argument_types = Some(types);
                    }
                }
                _ => {}
            }
        }
        spec
    }

    /// Every string in the attribute's fixed arguments, flattening a `params string[]`.
    fn read_strings(&self, row: u32) -> Vec<String> {
        let mut found = Vec::new();
        let Some(attr) = self.decode(row) else { return found };
        for (_, value) in &attr.fixed {
            match value {
                Value::Str(Some(s)) => found.push(s.clone()),
                Value::Array(Some(items)) => found.extend(items.iter().filter_map(|v| match v {
                    Value::Str(Some(s)) => Some(s.clone()),
                    _ => None,
                })),
                _ => {}
            }
        }
        found
    }

    /// The first integer argument — `[HarmonyPriority(Priority.First)]` and friends.
    fn read_int(&self, row: u32) -> Option<i32> {
        self.decode(row)?.fixed.iter().find_map(|(_, v)| match v {
            Value::Int32(i) => Some(*i),
            _ => None,
        })
    }

    // -- types and methods --------------------------------------------------------------------

    fn read_type(&mut self, ty: u32, harmony: bool) {
        let md = self.md;
        let Some(full) = md.type_def_name(ty) else { return };
        // Verse context, cheap and worth having even for assemblies with no Harmony in them.
        if md.type_def_base_name(ty).as_deref() == Some("Verse.Mod") {
            self.out.mod_classes.push(full.clone());
        }
        let mut class_spec = PatchSpec::default();
        let mut class_order = PatchOrdering::default();
        let mut class_has_patch = false;
        let mut patch_all = false;
        for row in self.type_attrs.get(&ty).cloned().unwrap_or_default() {
            match self.attribute_name(row).as_str() {
                "StaticConstructorOnStartupAttribute" | "StaticConstructorOnStartupPriorityAttribute" => self.out.startup_classes.push(full.clone()),
                "HarmonyPatchAttribute" => {
                    class_spec.apply(self.read_patch(row));
                    class_has_patch = true;
                }
                "HarmonyPatchAllAttribute" => {
                    patch_all = true;
                    class_has_patch = true;
                }
                "HarmonyPriorityAttribute" => class_order.priority = self.read_int(row),
                "HarmonyBeforeAttribute" => class_order.before = Some(self.read_strings(row)),
                "HarmonyAfterAttribute" => class_order.after = Some(self.read_strings(row)),
                _ => {}
            }
        }
        if !harmony {
            return;
        }
        for method in md.type_def_methods(ty) {
            self.read_method(method, &full, &class_spec, &class_order, class_has_patch, patch_all);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn read_method(&mut self, method: u32, declaring_type: &str, class_spec: &PatchSpec, class_order: &PatchOrdering, class_has_patch: bool, patch_all: bool) {
        let md = self.md;
        let Some(method_name) = md.method_name(method) else { return };
        let mut patch_all = patch_all;
        let mut spec = PatchSpec::default();
        let mut order = PatchOrdering::default();
        let mut kind: Option<&'static str> = None;
        let mut has_own_patch_attribute = false;
        let mut computes: Option<&'static str> = None;
        for row in self.method_attrs.get(&method).cloned().unwrap_or_default() {
            match self.attribute_name(row).as_str() {
                "HarmonyPatchAttribute" => {
                    spec.apply(self.read_patch(row));
                    has_own_patch_attribute = true;
                }
                "HarmonyPrefixAttribute" => kind = Some("prefix"),
                "HarmonyPostfixAttribute" => kind = Some("postfix"),
                "HarmonyTranspilerAttribute" => kind = Some("transpiler"),
                "HarmonyFinalizerAttribute" => kind = Some("finalizer"),
                "HarmonyReversePatchAttribute" => kind = Some("reverse"),
                "HarmonyPatchAllAttribute" => {
                    patch_all = true;
                    has_own_patch_attribute = true;
                }
                "HarmonyPriorityAttribute" => order.priority = self.read_int(row),
                "HarmonyBeforeAttribute" => order.before = Some(self.read_strings(row)),
                "HarmonyAfterAttribute" => order.after = Some(self.read_strings(row)),
                "HarmonyTargetMethodAttribute" | "HarmonyTargetMethodsAttribute" => computes = Some("TargetMethod() picks the patch target at runtime"),
                _ => {}
            }
        }
        // Harmony also finds patch methods by their name alone, inside a class that carries
        // [HarmonyPatch].
        if kind.is_none() && (class_has_patch || has_own_patch_attribute) {
            kind = kind_by_name(method_name).or_else(|| if has_own_patch_attribute { kind_by_suffix(method_name) } else { None });
        }
        if computes.is_some() || (matches!(method_name, "TargetMethod" | "TargetMethods") && class_has_patch) {
            self.add_manual(declaring_type, method_name, computes.unwrap_or("TargetMethod() picks the patch target at runtime"));
        }
        if let Some(kind) = kind {
            let mut merged = class_spec.clone();
            merged.apply(spec);
            let mut ordering = class_order.clone();
            ordering.apply(order);
            self.out.patches.push(PatchTarget {
                declaring_type: declaring_type.to_string(),
                method: method_name.to_string(),
                kind: kind.to_string(),
                target_type: merged.declaring_type,
                // [HarmonyPatchAll] means "every method of the target type"; "*" says that
                // plainly and keeps the field a string.
                target_method: merged.method_name.or_else(|| if patch_all { Some("*".to_string()) } else { None }),
                target_kind: method_type_name(merged.method_type).to_string(),
                argument_types: merged.argument_types,
                priority: ordering.priority,
                before: ordering.before.unwrap_or_default(),
                after: ordering.after.unwrap_or_default(),
                source: "attribute".to_string(),
            });
        }
        self.read_body(method, declaring_type, method_name);
    }

    // -- bodies -------------------------------------------------------------------------------

    /// Walk a method body for the things attributes cannot tell us: the Harmony id the mod
    /// registers itself under, the places it patches by hand, and the game methods it reaches
    /// for through `AccessTools`.
    fn read_body(&mut self, method: u32, declaring_type: &str, method_name: &str) {
        let md = self.md;
        let rva = md.method_rva(method).unwrap_or(0);
        if rva == 0 {
            return;
        }
        let Some(body) = il::method_body(self.pe, rva) else { return };
        let mut last_string: Option<String> = None;
        let mut pending_type: Option<String> = None;
        let mut resolved: VecDeque<(String, String)> = VecDeque::new();
        let mut reach = Reach::default();
        for (op, operand) in il::instructions(body) {
            if op == il::LDSTR {
                last_string = self.user_string(operand);
                reach.name.clone_from(&last_string);
                continue;
            }
            if op == il::LDTOKEN {
                // typeof(X) is `ldtoken X; call Type.GetTypeFromHandle`; the GetTypeFromHandle
                // call is left standing so the type survives until something consumes it.
                let named = self.token_type(operand);
                if !reach.in_array {
                    reach.ty = named.clone().or(reach.ty);
                }
                pending_type = named.or(pending_type);
                continue;
            }
            if op == il::NEWARR {
                reach.in_array = true;
                continue;
            }
            if op != il::CALL && op != il::CALLVIRT && op != il::NEWOBJ {
                continue;
            }
            let Some((ty, name)) = self.member(operand, 0) else {
                reach = Reach::default();
                continue;
            };
            // `typeof(X)` compiles to a call too; it is the one call that is part of the
            // arguments rather than the end of them.
            let arguments_end = name != "GetTypeFromHandle";
            let reach_now = if arguments_end { std::mem::take(&mut reach) } else { Reach::default() };
            if (op == il::NEWOBJ && name == ".ctor" && is_harmony_runner(&ty)) || (name == "Create" && is_harmony_runner(&ty)) {
                // `new Harmony("id")`, or Harmony 1.x's `HarmonyInstance.Create("id")`.
                if let Some(id) = last_string.take().filter(|s| !s.is_empty()) {
                    if !self.out.harmony_ids.contains(&id) {
                        self.out.harmony_ids.push(id);
                    }
                }
                continue;
            }
            if is_access_tools(&ty) || ty == "System.Type" || ty == "System.Reflection.TypeInfo" {
                if is_access_tools(&ty) {
                    self.note_reach(declaring_type, method_name, &name, reach_now.ty.as_deref(), reach_now.name.as_deref());
                }
                if let Some(target) = resolve_target(&name, &mut pending_type, &mut last_string) {
                    resolved.push_back(target);
                }
                continue;
            }
            if !is_harmony_runner(&ty) {
                continue;
            }
            match name.as_str() {
                "Patch" | "PatchCategory" | "CreateReversePatcher" | "ReversePatch" | "Unpatch" => {
                    let kind = match name.as_str() {
                        "CreateReversePatcher" | "ReversePatch" => "reverse",
                        "Unpatch" => "unpatch",
                        _ => "patch",
                    };
                    if let Some((target_type, target_method)) = resolved.pop_front() {
                        let target_kind = if target_method == ".ctor" { "constructor" } else { "normal" };
                        self.out.patches.push(PatchTarget {
                            declaring_type: declaring_type.to_string(),
                            method: method_name.to_string(),
                            kind: kind.to_string(),
                            target_type: Some(target_type),
                            target_method: Some(target_method),
                            target_kind: target_kind.to_string(),
                            source: "manual".to_string(),
                            ..Default::default()
                        });
                    } else {
                        self.add_manual(declaring_type, method_name, &format!("{name}(…) called with a computed target"));
                    }
                }
                "PatchAll" | "PatchAllUncategorized" => self.add_manual(declaring_type, method_name, "PatchAll() applies every [HarmonyPatch] in the assembly"),
                "UnpatchAll" | "UnpatchCategory" => self.add_manual(declaring_type, method_name, &format!("{name}() removes patches at runtime")),
                _ => {}
            }
        }
    }

    /// An `AccessTools.Method(typeof(X), "Name")` and its relatives, recorded as a method the
    /// caller reaches for. Frameworks patch through their own helpers, so this is often the
    /// only trace of what they touch.
    ///
    /// The type and name come from the `Reach` tracker rather than the sidecar's operands: the
    /// sidecar takes the last `ldtoken` before the call, which for
    /// `Method(typeof(SectionLayer), "RenderSpot", new[] { typeof(float) })` is `System.Single`.
    /// That answer is kept where the sidecar gave it, but these rows are new and can be right.
    fn note_reach(&mut self, declaring_type: &str, method_name: &str, call: &str, pending_type: Option<&str>, last_string: Option<&str>) {
        let (target_kind, needs_name) = match call {
            "Method" | "DeclaredMethod" | "Property" | "DeclaredProperty" => ("normal", true),
            "PropertyGetter" | "DeclaredPropertyGetter" => ("getter", true),
            "PropertySetter" | "DeclaredPropertySetter" => ("setter", true),
            "Constructor" | "DeclaredConstructor" => ("constructor", false),
            _ => return,
        };
        let Some(ty) = pending_type else { return };
        let target_method = if needs_name {
            match last_string {
                Some(name) if !name.is_empty() => name.to_string(),
                _ => return,
            }
        } else {
            ".ctor".to_string()
        };
        let key = (declaring_type.to_string(), method_name.to_string(), ty.to_string(), target_method.clone(), target_kind);
        if !self.seen_reach.insert(key) {
            return;
        }
        self.out.patches.push(PatchTarget {
            declaring_type: declaring_type.to_string(),
            method: method_name.to_string(),
            kind: "reaches".to_string(),
            target_type: Some(ty.to_string()),
            target_method: Some(target_method),
            target_kind: target_kind.to_string(),
            source: "accesstools".to_string(),
            ..Default::default()
        });
    }

    fn add_manual(&mut self, declaring_type: &str, method: &str, detail: &str) {
        if !self.seen_manual.insert((declaring_type.to_string(), method.to_string(), detail.to_string())) {
            return;
        }
        self.out.manual_patches.push(ManualPatch { declaring_type: declaring_type.to_string(), method: method.to_string(), detail: detail.to_string() });
    }

    // -- tokens -------------------------------------------------------------------------------

    /// The user string a `ldstr` token points at.
    fn user_string(&self, token: u32) -> Option<String> {
        if token >> 24 != 0x70 {
            return None;
        }
        self.md.user_string(token & 0x00FF_FFFF)
    }

    /// Declaring type and member name behind a call/newobj token. A member of a constructed
    /// type (a TypeSpec parent) has no name here, as in the sidecar, so such calls are skipped.
    fn member(&self, token: u32, depth: u32) -> Option<(String, String)> {
        let md = self.md;
        let row = token & 0x00FF_FFFF;
        match token >> 24 {
            0x0A => {
                let (table, parent) = md.member_ref_parent(row)?;
                Some((md.type_name(table, parent)?, md.member_ref_name(row)?.to_string()))
            }
            0x06 => Some((md.type_def_name(md.method_owner(row)?)?, md.method_name(row)?.to_string())),
            0x2B if depth == 0 => {
                let (table, target) = md.method_spec_method(row)?;
                let token = ((table as u32) << 24) | target;
                self.member(token, depth + 1)
            }
            _ => None,
        }
    }

    /// The type a `ldtoken` pushes, or nothing when it pushes a method, a field or a
    /// constructed type.
    fn token_type(&self, token: u32) -> Option<String> {
        let row = token & 0x00FF_FFFF;
        match token >> 24 {
            0x01 => self.md.type_ref_name(row),
            0x02 => self.md.type_def_name(row),
            _ => None,
        }
    }
}

/// The elements of a `Type[]` attribute argument, or nothing when the array is of something
/// else (`ArgumentType[]` arrives here too and must not be mistaken for a signature).
fn type_array(items: &[Value]) -> Option<Vec<String>> {
    items
        .iter()
        .map(|item| match item {
            Value::Type(Some(s)) => Some(clean(s)),
            Value::Type(None) => Some("null".to_string()),
            _ => None,
        })
        .collect()
}

/// Turn the reflection call the mod just made into a (type, method) pair, consuming the
/// operands it used. Nothing when the call was not one that names a target.
fn resolve_target(name: &str, pending_type: &mut Option<String>, last_string: &mut Option<String>) -> Option<(String, String)> {
    match name {
        "Constructor" | "DeclaredConstructor" | "GetConstructor" => pending_type.take().map(|ty| (ty, ".ctor".to_string())),
        "Method" | "DeclaredMethod" | "GetMethod" | "PropertyGetter" | "DeclaredPropertyGetter" | "PropertySetter" | "DeclaredPropertySetter" | "GetProperty" | "EnumeratorMoveNext" => {
            // AccessTools.Method("Verse.Pawn:Tick") names both halves in one string.
            if pending_type.is_none() {
                if let Some(s) = last_string.as_deref().filter(|s| s.contains(':')) {
                    let (ty, method) = s.split_once(':').unwrap_or((s, ""));
                    let target = if ty.is_empty() || method.is_empty() { None } else { Some((ty.to_string(), method.to_string())) };
                    *last_string = None;
                    return target;
                }
            }
            let ty = pending_type.as_ref()?;
            let method = last_string.as_ref()?;
            let target = (ty.clone(), method.clone());
            *pending_type = None;
            *last_string = None;
            Some(target)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_type_names_lose_their_assembly() {
        assert_eq!(clean("RimWorld.Pawn, Assembly-CSharp, Version=1.0"), "RimWorld.Pawn");
        assert_eq!(clean("System.Collections.Generic.List`1[[System.Int32, mscorlib]], mscorlib"), "System.Collections.Generic.List`1[[System.Int32, mscorlib]]");
        assert_eq!(clean(" Verse.Pawn "), "Verse.Pawn");
    }

    #[test]
    fn kinds_by_name_and_suffix() {
        assert_eq!(kind_by_name("prefix"), Some("prefix"));
        assert_eq!(kind_by_name("Pawn_Tick_Prefix"), None);
        assert_eq!(kind_by_suffix("Pawn_Tick_Prefix"), Some("prefix"));
        assert_eq!(kind_by_suffix("Prefix"), None, "the bare word is not a suffix");
        assert_eq!(kind_by_suffix("Helper"), None);
    }

    #[test]
    fn a_colon_string_names_both_halves() {
        let mut ty = None;
        let mut s = Some("Verse.Pawn:Tick".to_string());
        assert_eq!(resolve_target("Method", &mut ty, &mut s), Some(("Verse.Pawn".into(), "Tick".into())));
        assert!(s.is_none());
        let mut s = Some("Verse.Pawn:".to_string());
        assert_eq!(resolve_target("Method", &mut ty, &mut s), None);
        let mut ty = Some("Verse.Pawn".to_string());
        let mut s = Some("Verse.Pawn:Tick".to_string());
        assert_eq!(resolve_target("Method", &mut ty, &mut s), Some(("Verse.Pawn".into(), "Verse.Pawn:Tick".into())), "a pending type wins over the colon form");
        assert_eq!(resolve_target("Constructor", &mut Some("X".into()), &mut None), Some(("X".into(), ".ctor".into())));
        assert_eq!(resolve_target("Field", &mut Some("X".into()), &mut Some("f".into())), None);
    }
}
