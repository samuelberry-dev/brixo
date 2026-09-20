use std::collections::HashMap;

/// Unique, never-reused identifier for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(u64);

/// What kind of thing an instance is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Workspace,
    Folder,
    Part,
}

#[derive(Debug)]
pub struct Instance {
    pub id: InstanceId,
    pub class: Class,
    pub name: String,
    pub parent: Option<InstanceId>,
    pub children: Vec<InstanceId>,
}

/// Owns every instance in the tree. The Workspace is the root.
pub struct DataModel {
    instances: HashMap<InstanceId, Instance>,
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

    /// Creates an instance under `parent`. Returns None if the parent doesn't exist.
    pub fn create(&mut self, class: Class, name: &str, parent: InstanceId) -> Option<InstanceId> {
        if !self.instances.contains_key(&parent) {
            return None;
        }
        let id = InstanceId(self.next_id);
        self.next_id += 1;
        self.instances.insert(
            id,
            Instance {
                id,
                class,
                name: name.to_string(),
                parent: Some(parent),
                children: Vec::new(),
            },
        );
        if let Some(p) = self.instances.get_mut(&parent) {
            p.children.push(id);
        }
        Some(id)
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
    /// `id` is the root, or the move would create a cycle.
    pub fn reparent(&mut self, id: InstanceId, new_parent: InstanceId) -> bool {
        if id == self.root || id == new_parent {
            return false;
        }
        if !self.instances.contains_key(&id) || !self.instances.contains_key(&new_parent) {
            return false;
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
}