use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique, never-reused identifier for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId(u64);

impl InstanceId {
    /// The raw number, for handing to scripts.
    pub fn raw(self) -> u64 {
        self.0
    }

    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Vec3 = Vec3 {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// 8-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Properties that only a Part has.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PartProps {
    pub position: Vec3,
    pub size: Vec3,
    /// Euler angles in degrees.
    pub rotation: Vec3,
    pub color: Color,
}

impl Default for PartProps {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            size: Vec3::ONE,
            rotation: Vec3::ZERO,
            color: Color::new(160, 160, 160),
        }
    }
}

/// A Rovik script. It runs when the game plays; `self` is its parent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptProps {
    pub source: String,
    #[serde(default = "enabled_default")]
    pub enabled: bool,
}

fn enabled_default() -> bool {
    true
}

impl Default for ScriptProps {
    fn default() -> Self {
        Self {
            source: "-- This script runs when you press Play.\n-- 'self' is the part it's inside.\n\nprint(\"Hello from \" + self.name)\n".to_string(),
            enabled: true,
        }
    }
}

/// What kind of thing an instance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Class {
    Workspace,
    Folder,
    Part,
    Script,
}

impl Class {
    /// Whether this kind of instance may have children.
    pub fn can_hold_children(self) -> bool {
        !matches!(self, Class::Script)
    }
}

/// Per-class data. Kept in sync with `Instance::class` by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Props {
    Workspace,
    Folder,
    Part(PartProps),
    Script(ScriptProps),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: InstanceId,
    pub class: Class,
    pub name: String,
    pub parent: Option<InstanceId>,
    pub children: Vec<InstanceId>,
    pub props: Props,
}

/// Owns every instance in the tree. The Workspace is the root.
#[derive(Debug, Clone)]
pub struct DataModel {
    instances: HashMap<InstanceId, Instance>,
    next_id: u64,
    root: InstanceId,
}

/// Flat, serialisable form of the tree. Maps with non-string keys don't
/// round-trip through JSON, so instances are stored as a list.
#[derive(Serialize, Deserialize)]
struct SavedModel {
    instances: Vec<Instance>,
    next_id: u64,
    root: InstanceId,
}

impl DataModel {
    pub fn new() -> Self {
        let root = InstanceId(0);
        let mut instances = HashMap::new();
        instances.insert(
            root,
            Instance {
                id: root,
                class: Class::Workspace,
                name: "Workspace".to_string(),
                parent: None,
                children: Vec::new(),
                props: Props::Workspace,
            },
        );
        Self {
            instances,
            next_id: 1,
            root,
        }
    }

    pub fn root(&self) -> InstanceId {
        self.root
    }

    pub fn get(&self, id: InstanceId) -> Option<&Instance> {
        self.instances.get(&id)
    }

    pub fn get_mut(&mut self, id: InstanceId) -> Option<&mut Instance> {
        self.instances.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Every instance, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = &Instance> {
        self.instances.values()
    }

    /// Every instance in tree order, starting at the root.
    pub fn walk(&self) -> Vec<InstanceId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(id) = stack.pop() {
            if let Some(inst) = self.instances.get(&id) {
                out.push(id);
                stack.extend(inst.children.iter().rev().copied());
            }
        }
        out
    }

    /// The first instance with this name, searching in tree order.
    pub fn find_first(&self, name: &str) -> Option<InstanceId> {
        self.walk()
            .into_iter()
            .find(|id| self.instances[id].name == name)
    }

    /// Read a Part's properties. None if the id is missing or isn't a Part.
    pub fn part(&self, id: InstanceId) -> Option<&PartProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Part(p)) => Some(p),
            _ => None,
        }
    }

    /// Mutate a Part's properties. None if the id is missing or isn't a Part.
    pub fn part_mut(&mut self, id: InstanceId) -> Option<&mut PartProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Part(p)) => Some(p),
            _ => None,
        }
    }

    /// Read a Script's properties. None if the id is missing or isn't a Script.
    pub fn script(&self, id: InstanceId) -> Option<&ScriptProps> {
        match self.instances.get(&id).map(|i| &i.props) {
            Some(Props::Script(s)) => Some(s),
            _ => None,
        }
    }

    pub fn script_mut(&mut self, id: InstanceId) -> Option<&mut ScriptProps> {
        match self.instances.get_mut(&id).map(|i| &mut i.props) {
            Some(Props::Script(s)) => Some(s),
            _ => None,
        }
    }

    /// Creates an instance under `parent`. Returns None if the parent doesn't
    /// exist or can't hold children.
    pub fn create(&mut self, class: Class, name: &str, parent: InstanceId) -> Option<InstanceId> {
        let parent_class = self.instances.get(&parent)?.class;
        if !parent_class.can_hold_children() {
            return None;
        }
        let id = InstanceId(self.next_id);
        self.next_id += 1;
        let props = match class {
            Class::Workspace => Props::Workspace,
            Class::Folder => Props::Folder,
            Class::Part => Props::Part(PartProps::default()),
            Class::Script => Props::Script(ScriptProps::default()),
        };
        self.instances.insert(
            id,
            Instance {
                id,
                class,
                name: name.to_string(),
                parent: Some(parent),
                children: Vec::new(),
                props,
            },
        );
        if let Some(p) = self.instances.get_mut(&parent) {
            p.children.push(id);
        }
        Some(id)
    }

    /// Copies an instance and everything inside it, placing the copy next to
    /// the original. Returns the copy's id.
    pub fn clone_subtree(&mut self, id: InstanceId) -> Option<InstanceId> {
        if id == self.root {
            return None;
        }
        let parent = self.instances.get(&id)?.parent?;
        let copy = self.copy_into(id, parent)?;
        Some(copy)
    }

    fn copy_into(&mut self, source: InstanceId, parent: InstanceId) -> Option<InstanceId> {
        let original = self.instances.get(&source)?.clone();
        let new_id = InstanceId(self.next_id);
        self.next_id += 1;
        self.instances.insert(
            new_id,
            Instance {
                id: new_id,
                class: original.class,
                name: original.name.clone(),
                parent: Some(parent),
                children: Vec::new(),
                props: original.props.clone(),
            },
        );
        if let Some(p) = self.instances.get_mut(&parent) {
            p.children.push(new_id);
        }
        for child in original.children {
            self.copy_into(child, new_id);
        }
        Some(new_id)
    }

    /// Removes an instance and all its descendants.
    /// Returns false if it doesn't exist or is the root.
    pub fn remove(&mut self, id: InstanceId) -> bool {
        if id == self.root || !self.instances.contains_key(&id) {
            return false;
        }

        // Detach from the parent's child list.
        let parent_id = self.instances[&id].parent;
        if let Some(pid) = parent_id {
            if let Some(p) = self.instances.get_mut(&pid) {
                p.children.retain(|c| *c != id);
            }
        }

        // Delete the whole subtree.
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(inst) = self.instances.remove(&current) {
                stack.extend(inst.children);
            }
        }
        true
    }

    /// True if `node` sits somewhere below `ancestor` in the tree.
    fn is_descendant_of(&self, node: InstanceId, ancestor: InstanceId) -> bool {
        let mut current = self.instances.get(&node).and_then(|i| i.parent);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.instances.get(&id).and_then(|i| i.parent);
        }
        false
    }

    /// Moves `id` under `new_parent`. Returns false if either doesn't exist,
    /// `id` is the root, the new parent can't hold children, or the move
    /// would create a cycle.
    pub fn reparent(&mut self, id: InstanceId, new_parent: InstanceId) -> bool {
        if id == self.root || id == new_parent {
            return false;
        }
        if !self.instances.contains_key(&id) {
            return false;
        }
        match self.instances.get(&new_parent) {
            Some(p) if p.class.can_hold_children() => {}
            _ => return false,
        }
        if self.is_descendant_of(new_parent, id) {
            return false;
        }

        let old_parent = self.instances[&id].parent;
        if let Some(op) = old_parent {
            if let Some(p) = self.instances.get_mut(&op) {
                p.children.retain(|c| *c != id);
            }
        }
        self.instances
            .get_mut(&new_parent)
            .unwrap()
            .children
            .push(id);
        self.instances.get_mut(&id).unwrap().parent = Some(new_parent);
        true
    }

    // --- saving and loading ---

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut instances: Vec<Instance> = self.instances.values().cloned().collect();
        // Sort so saved files are stable rather than in random map order.
        instances.sort_by_key(|i| i.id.0);
        let saved = SavedModel {
            instances,
            next_id: self.next_id,
            root: self.root,
        };
        serde_json::to_string_pretty(&saved)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let saved: SavedModel = serde_json::from_str(json)?;
        let mut instances = HashMap::new();
        for inst in saved.instances {
            instances.insert(inst.id, inst);
        }
        Ok(Self {
            instances,
            next_id: saved.next_id,
            root: saved.root,
        })
    }

    pub fn save_file(&self, path: &str) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    pub fn load_file(path: &str) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

impl Default for DataModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tree_with_correct_links() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();

        assert_eq!(dm.get(folder).unwrap().parent, Some(workspace));
        assert_eq!(dm.get(part).unwrap().parent, Some(folder));
        assert_eq!(dm.get(workspace).unwrap().children, vec![folder]);
        assert_eq!(dm.get(folder).unwrap().children, vec![part]);
    }

    #[test]
    fn create_under_missing_parent_returns_none() {
        let mut dm = DataModel::new();
        let bogus = InstanceId(999);
        assert!(dm.create(Class::Part, "Lost", bogus).is_none());
    }

    #[test]
    fn remove_deletes_the_whole_subtree() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();

        assert!(dm.remove(folder));
        assert!(dm.get(folder).is_none());
        assert!(dm.get(part).is_none());
        assert!(dm.get(workspace).unwrap().children.is_empty());
    }

    #[test]
    fn cannot_remove_the_root_or_a_missing_instance() {
        let mut dm = DataModel::new();
        let root = dm.root();
        assert!(!dm.remove(root));
        assert!(!dm.remove(InstanceId(999)));
    }

    #[test]
    fn reparent_moves_an_instance() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Folder, "A", workspace).unwrap();
        let b = dm.create(Class::Folder, "B", workspace).unwrap();
        let part = dm.create(Class::Part, "Box", a).unwrap();

        assert!(dm.reparent(part, b));
        assert_eq!(dm.get(part).unwrap().parent, Some(b));
        assert!(dm.get(a).unwrap().children.is_empty());
        assert_eq!(dm.get(b).unwrap().children, vec![part]);
    }

    #[test]
    fn reparent_rejects_cycles() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Folder, "A", workspace).unwrap();
        let b = dm.create(Class::Folder, "B", a).unwrap();

        assert!(!dm.reparent(a, b)); // can't move a folder into its own child
        assert!(!dm.reparent(a, a)); // can't move something into itself
    }

    #[test]
    fn parts_get_default_props_and_can_be_edited() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let part = dm.create(Class::Part, "Box", workspace).unwrap();

        assert_eq!(dm.part(part).unwrap().size, Vec3::ONE);

        let p = dm.part_mut(part).unwrap();
        p.position = Vec3::new(1.0, 5.0, -3.0);
        p.color = Color::new(255, 0, 0);

        assert_eq!(dm.part(part).unwrap().position, Vec3::new(1.0, 5.0, -3.0));
        assert_eq!(dm.part(part).unwrap().color, Color::new(255, 0, 0));
    }

    #[test]
    fn non_parts_have_no_part_props() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Stuff", workspace).unwrap();

        assert!(dm.part(folder).is_none());
        assert!(dm.part(workspace).is_none());
    }

    #[test]
    fn json_round_trip_preserves_the_tree() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let folder = dm.create(Class::Folder, "Enemies", workspace).unwrap();
        let part = dm.create(Class::Part, "Goblin", folder).unwrap();
        dm.part_mut(part).unwrap().position = Vec3::new(2.0, 0.5, 7.0);

        let json = dm.to_json().unwrap();
        let loaded = DataModel::from_json(&json).unwrap();

        assert_eq!(loaded.len(), dm.len());
        assert_eq!(loaded.root(), workspace);
        assert_eq!(loaded.get(part).unwrap().name, "Goblin");
        assert_eq!(loaded.get(part).unwrap().parent, Some(folder));
        assert_eq!(loaded.get(folder).unwrap().children, vec![part]);
        assert_eq!(loaded.part(part).unwrap().position, Vec3::new(2.0, 0.5, 7.0));
    }

    #[test]
    fn loaded_model_keeps_handing_out_fresh_ids() {
        let mut dm = DataModel::new();
        let workspace = dm.root();
        let a = dm.create(Class::Part, "A", workspace).unwrap();

        let mut loaded = DataModel::from_json(&dm.to_json().unwrap()).unwrap();
        let b = loaded.create(Class::Part, "B", workspace).unwrap();

        assert_ne!(a, b);
        assert!(loaded.get(a).is_some());
        assert!(loaded.get(b).is_some());
    }

    #[test]
    fn scripts_hold_source_and_round_trip() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let part = dm.create(Class::Part, "Coin", root).unwrap();
        let script = dm.create(Class::Script, "Spin", part).unwrap();
        dm.script_mut(script).unwrap().source = "print(1)".to_string();

        let loaded = DataModel::from_json(&dm.to_json().unwrap()).unwrap();
        assert_eq!(loaded.script(script).unwrap().source, "print(1)");
        assert!(loaded.script(script).unwrap().enabled);
        assert!(loaded.part(script).is_none());
    }

    #[test]
    fn scripts_cannot_hold_children() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let script = dm.create(Class::Script, "S", root).unwrap();
        let folder = dm.create(Class::Folder, "F", root).unwrap();
        assert!(dm.create(Class::Part, "Inside", script).is_none());
        assert!(!dm.reparent(folder, script));
    }

    #[test]
    fn old_saves_without_enabled_still_load() {
        let json = r#"{"instances":[
            {"id":0,"class":"Workspace","name":"Workspace","parent":null,"children":[1],"props":"Workspace"},
            {"id":1,"class":"Script","name":"S","parent":0,"children":[],"props":{"Script":{"source":"x = 1"}}}
        ],"next_id":2,"root":0}"#;
        let dm = DataModel::from_json(json).unwrap();
        assert!(dm.script(InstanceId(1)).unwrap().enabled);
    }

    #[test]
    fn clone_subtree_copies_children_with_new_ids() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let part = dm.create(Class::Part, "Coin", root).unwrap();
        let script = dm.create(Class::Script, "Spin", part).unwrap();
        dm.part_mut(part).unwrap().position = Vec3::new(1.0, 2.0, 3.0);

        let copy = dm.clone_subtree(part).unwrap();
        assert_ne!(copy, part);
        assert_eq!(dm.get(copy).unwrap().parent, Some(root));
        assert_eq!(dm.part(copy).unwrap().position, Vec3::new(1.0, 2.0, 3.0));
        let copied_children = &dm.get(copy).unwrap().children;
        assert_eq!(copied_children.len(), 1);
        assert_ne!(copied_children[0], script);
        assert!(dm.script(copied_children[0]).is_some());
    }

    #[test]
    fn find_first_searches_in_tree_order() {
        let mut dm = DataModel::new();
        let root = dm.root();
        let folder = dm.create(Class::Folder, "F", root).unwrap();
        let deep = dm.create(Class::Part, "Target", folder).unwrap();
        let _later = dm.create(Class::Part, "Target", root).unwrap();
        assert_eq!(dm.find_first("Target"), Some(deep));
        assert_eq!(dm.find_first("Nope"), None);
    }
}
