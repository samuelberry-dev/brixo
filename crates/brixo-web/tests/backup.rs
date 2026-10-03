//! Backups: a safe copy of the live database, even while it's being written.

use brixo_web::db::{backup, Db};

#[test]
fn a_backup_is_a_full_checked_copy_and_never_overwrites() {
    let dir = std::env::temp_dir().join(format!("brixo-backup-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let live = dir.join("live.sqlite");
    let db = Db::open(live.to_str().unwrap()).unwrap();
    db.create_user("ann", "hash").unwrap();
    db.create_user("bob", "hash").unwrap();

    let copy = dir.join("copy.sqlite");
    let (users, _games) = backup(live.to_str().unwrap(), copy.to_str().unwrap()).unwrap();
    assert_eq!(users, 2);
    // The copy is a working database in its own right.
    let restored = Db::open(copy.to_str().unwrap()).unwrap();
    assert!(restored.login_info("ann").unwrap().is_some());
    // Changes after the backup aren't in it.
    db.create_user("cleo", "hash").unwrap();
    assert!(restored.login_info("cleo").unwrap().is_none());
    // It won't write over an existing file.
    assert!(backup(live.to_str().unwrap(), copy.to_str().unwrap()).is_err());
    // Nor make an empty database out of a wrong path.
    assert!(backup(dir.join("nope.sqlite").to_str().unwrap(), dir.join("x.sqlite").to_str().unwrap()).is_err());
    assert!(!dir.join("nope.sqlite").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
