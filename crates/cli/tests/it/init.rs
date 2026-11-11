//! `bt-image init`.

use crate::common::{Outcome, bt, folder};

#[test]
fn init_creates_a_project_prints_its_file_and_refuses_to_start_over() -> Outcome {
    let root = folder("init")?;
    let target = root.join("new");
    let ran = bt(&["init", &target.to_string_lossy()]);
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.trim(), target.join("bt-image.toml").to_string_lossy());
    assert!(target.join("photos").is_dir() && target.join("recipes/glitch.toml").is_file());
    let again = bt(&["init", &target.to_string_lossy()]);
    assert_eq!(again.code, 1);
    assert!(
        again.err.starts_with("error: ") && again.err.contains("bt-image.toml already exists"),
        "{}",
        again.err
    );
    Ok(())
}
