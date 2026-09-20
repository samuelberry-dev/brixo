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
        Self { instances, next_id: 1, root }
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
}