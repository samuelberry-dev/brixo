//! Copying things out of one game and pasting them into another.

use brixo_core::{Attribute, Class, DataModel, Vec3};

#[test]
fn a_model_with_a_script_goes_from_one_game_to_another() {
    let mut a = DataModel::new();
    let root = a.root();
    let tree = a.create(Class::Model, "Tree", root).unwrap();
    let trunk = a.create(Class::Part, "Trunk", tree).unwrap();
    a.part_mut(trunk).unwrap().position = Vec3::new(3.0, 4.0, 5.0);
    let script = a.create(Class::Script, "Sway", trunk).unwrap();
    a.script_mut(script).unwrap().source = "print(\"hi\")".into();
    a.get_mut(trunk).unwrap().attributes.insert("wood".into(), Attribute::Num(7.0));
    let text = a.to_clipboard(&[tree]);

    // Another game, with things of its own already there.
    let mut b = DataModel::new();
    let broot = b.root();
    for i in 0..5 {
        b.create(Class::Part, &format!("Filler{i}"), broot).unwrap();
    }
    let pasted = b.paste_clipboard(&text, broot).expect("it's Brixo's clipboard");
    assert_eq!(pasted.len(), 1);
    let new_tree = pasted[0];
    assert_ne!(new_tree, tree, "fresh ids");
    let inst = b.get(new_tree).unwrap();
    assert_eq!((inst.name.as_str(), inst.class), ("Tree", Class::Model));
    let new_trunk = inst.children[0];
    assert_eq!(b.part(new_trunk).unwrap().position, Vec3::new(3.0, 4.0, 5.0));
    assert_eq!(b.get(new_trunk).unwrap().attributes.get("wood"), Some(&Attribute::Num(7.0)));
    let new_script = b.get(new_trunk).unwrap().children[0];
    assert_eq!(b.script(new_script).unwrap().source, "print(\"hi\")");
    assert_eq!(b.get(new_script).unwrap().parent, Some(new_trunk));
    // Pasting twice makes two copies.
    b.paste_clipboard(&text, broot).unwrap();
    assert_eq!(b.walk().iter().filter(|id| b.get(**id).unwrap().name == "Tree").count(), 2);
}

#[test]
fn copying_something_and_its_child_copies_it_once() {
    let mut a = DataModel::new();
    let root = a.root();
    let folder = a.create(Class::Folder, "Stuff", root).unwrap();
    let part = a.create(Class::Part, "Brick", folder).unwrap();
    let text = a.to_clipboard(&[folder, part]);
    let mut b = DataModel::new();
    let broot = b.root();
    let pasted = b.paste_clipboard(&text, broot).unwrap();
    assert_eq!(pasted.len(), 1, "just the folder (the brick comes inside it)");
    assert_eq!(b.walk().iter().filter(|id| b.get(**id).unwrap().name == "Brick").count(), 1);
}

#[test]
fn other_text_and_bad_places_are_refused() {
    let mut a = DataModel::new();
    let root = a.root();
    let part = a.create(Class::Part, "P", root).unwrap();
    assert!(a.paste_clipboard("just some text someone copied", root).is_none());
    assert!(a.paste_clipboard("{\"brixo_clipboard\": 99, \"items\": []}", root).is_none());
    let text = a.to_clipboard(&[part]);
    // A Script can't hold a part.
    let s = a.create(Class::Script, "S", root).unwrap();
    assert!(a.paste_clipboard(&text, s).is_none());
    // The Workspace itself can't be copied.
    assert_eq!(a.to_clipboard(&[root]), "{\"brixo_clipboard\":1,\"items\":[]}");
}
